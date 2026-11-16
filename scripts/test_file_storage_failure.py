#!/usr/bin/env python3
"""Verify file storage failures do not disable Lemmy's pict-rs image API."""

import json
import os
import struct
import urllib.error
import urllib.request
import zlib


BASE_URL = os.environ.get("LEMMY_TEST_URL", "http://localhost:8536").rstrip("/")
HOSTNAME = os.environ.get("LEMMY_TEST_HOSTNAME", "localhost")
ADMIN_USERNAME = os.environ.get("LEMMY_TEST_ADMIN_USERNAME", "admin")
ADMIN_PASSWORD = os.environ.get("LEMMY_TEST_ADMIN_PASSWORD", "lemmy-test-password")
FAILURE_MODE = os.environ.get("LEMMY_TEST_FAILURE_MODE", "missing")


def request(path, method="GET", body=None, headers=None):
    request_headers = {"Host": HOSTNAME}
    request_headers.update(headers or {})
    req = urllib.request.Request(
        BASE_URL + path,
        data=body,
        method=method,
        headers=request_headers,
    )
    try:
        with urllib.request.urlopen(req, timeout=45) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        response = error.code, error.read()
        error.close()
        return response


status, body = request(
    "/api/v4/account/auth/login",
    "POST",
    json.dumps(
        {
            "username_or_email": ADMIN_USERNAME,
            "password": ADMIN_PASSWORD,
        }
    ).encode(),
    {"Content-Type": "application/json"},
)
assert status == 200, f"admin login failed: HTTP {status}: {body[:200]!r}"
token = json.loads(body)["jwt"]
auth = {"Authorization": f"Bearer {token}"}

boundary = "lemmy-file-storage-failure-boundary"
document = (
    f"--{boundary}\r\n"
    'Content-Disposition: form-data; name="file"; filename="offline.pdf"\r\n'
    "Content-Type: application/pdf\r\n\r\n"
    "%PDF-1.7\nS3 failure fixture\n\r\n"
    f"--{boundary}--\r\n"
).encode()
status, body = request(
    "/api/v4/file",
    "POST",
    document,
    {**auth, "Content-Type": f"multipart/form-data; boundary={boundary}"},
)
if FAILURE_MODE == "missing":
    assert status == 400, f"missing S3 config should disable documents: HTTP {status}: {body[:200]!r}"
elif FAILURE_MODE == "unavailable":
    assert status == 503, f"unavailable S3 should return 503: HTTP {status}: {body[:200]!r}"
else:
    raise ValueError("LEMMY_TEST_FAILURE_MODE must be missing or unavailable")

status, body = request("/api/v4/file/list", headers=auth)
assert status == 200, f"file list failed: HTTP {status}: {body[:200]!r}"
assert json.loads(body)["files"] == [], "failed uploads must not create file references"

def png_chunk(kind, data):
    payload = kind + data
    return struct.pack(">I", len(data)) + payload + struct.pack(">I", zlib.crc32(payload))


png = (
    b"\x89PNG\r\n\x1a\n"
    + png_chunk(b"IHDR", struct.pack(">2I5B", 1, 1, 8, 2, 0, 0, 0))
    + png_chunk(b"IDAT", zlib.compress(b"\x00\xff\x00\x00"))
    + png_chunk(b"IEND", b"")
)
image_boundary = "lemmy-image-api-failure-check"
image_body = (
    f"--{image_boundary}\r\n"
    'Content-Disposition: form-data; name="images[]"; filename="pixel.png"\r\n'
    "Content-Type: image/png\r\n\r\n"
).encode() + png + f"\r\n--{image_boundary}--\r\n".encode()
status, body = request(
    "/api/v4/image",
    "POST",
    image_body,
    {**auth, "Content-Type": f"multipart/form-data; boundary={image_boundary}"},
)
assert status == 200, f"pict-rs image API should remain available: HTTP {status}: {body[:200]!r}"
print(f"PASS {FAILURE_MODE} S3 failure leaves document references empty and image upload available")
