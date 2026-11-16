#!/usr/bin/env python3
"""HTTP integration checks for generic file storage against a running Lemmy/RustFS stack."""

import json
import os
import unittest
import urllib.error
import urllib.request
import uuid
from urllib.parse import urlsplit


BASE_URL = os.environ.get("LEMMY_TEST_URL", "http://localhost:8536").rstrip("/")
HOSTNAME = os.environ.get("LEMMY_TEST_HOSTNAME", "localhost")
ADMIN_USERNAME = os.environ.get("LEMMY_TEST_ADMIN_USERNAME", "admin")
ADMIN_PASSWORD = os.environ.get("LEMMY_TEST_ADMIN_PASSWORD", "lemmy-test-password")
INSTALLATION_ADMIN_USERNAME = os.environ.get(
    "LEMMY_TEST_INSTALLATION_ADMIN_USERNAME", "lemmy-test-installation-admin"
)
INSTALLATION_ADMIN_PASSWORD = os.environ.get(
    "LEMMY_TEST_INSTALLATION_ADMIN_PASSWORD", "lemmy-test-installation-password"
)
BOUNDARY = "lemmy-file-storage-integration-boundary"


def request(path, method="GET", body=None, headers=None, host=None):
    request_headers = {"Host": host or HOSTNAME}
    request_headers.update(headers or {})
    req = urllib.request.Request(
        BASE_URL + path,
        data=body,
        method=method,
        headers=request_headers,
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            return response.status, response.read(), response.headers
    except urllib.error.HTTPError as error:
        response = error.code, error.read(), error.headers
        error.close()
        return response


def multipart(filename, contents):
    headers = (
        f"--{BOUNDARY}\r\n"
        f'Content-Disposition: form-data; name="file"; filename="{filename}"\r\n'
        "Content-Type: application/octet-stream\r\n\r\n"
    ).encode()
    return headers + contents + f"\r\n--{BOUNDARY}--\r\n".encode()


class FileStorageIntegration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        status, body, _ = request(
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
        if status != 200:
            raise RuntimeError(f"test admin login failed: HTTP {status}: {body[:200]!r}")
        cls.token = json.loads(body)["jwt"]
        cls.auth = {"Authorization": f"Bearer {cls.token}"}
        cls.edit_site(
            {
                "file_upload_disabled": False,
                "file_max_upload_size_bytes": 1048576,
                "file_allowed_mime_types": ["application/pdf"],
                "private_instance": False,
                "rate_limit_image_max_requests": 100,
                "rate_limit_image_interval_seconds": 60,
                "rate_limit_message_max_requests": 1000,
                "rate_limit_message_interval_seconds": 60,
            }
        )

        status, body, _ = request(
            "/api/v4/installation/login",
            "POST",
            json.dumps(
                {
                    "username": INSTALLATION_ADMIN_USERNAME,
                    "password": INSTALLATION_ADMIN_PASSWORD,
                }
            ).encode(),
            {"Content-Type": "application/json"},
        )
        if status != 200:
            raise RuntimeError(f"installation admin login failed: HTTP {status}: {body[:200]!r}")
        cls.installation_auth = {
            "Authorization": f"Bearer {json.loads(body)['jwt']}"
        }
        cls.second_tenant_hostname = f"file-test-{uuid.uuid4().hex}.localhost"
        cls.second_admin_username = f"file-test-{uuid.uuid4().hex[:12]}"
        cls.second_admin_password = uuid.uuid4().hex
        status, body, _ = request(
            "/api/v4/installation/sites",
            "POST",
            json.dumps(
                {
                    "hostname": cls.second_tenant_hostname,
                    "site_name": "File isolation test",
                    "admin_username": cls.second_admin_username,
                    "admin_password": cls.second_admin_password,
                }
            ).encode(),
            {**cls.installation_auth, "Content-Type": "application/json"},
        )
        if status != 200:
            raise RuntimeError(f"creating second test tenant failed: HTTP {status}: {body[:200]!r}")
        cls.second_tenant_id = json.loads(body)["local_site"]["id"]

    @classmethod
    def tearDownClass(cls):
        if hasattr(cls, "second_tenant_id"):
            request(
                f"/api/v4/installation/sites/{cls.second_tenant_id}",
                "DELETE",
                headers=cls.installation_auth,
            )

    @classmethod
    def edit_site(cls, settings):
        status, body, _ = request(
            "/api/v4/site",
            "PUT",
            json.dumps(settings).encode(),
            {**cls.auth, "Content-Type": "application/json"},
        )
        if status != 200:
            raise RuntimeError(f"updating test site settings failed: HTTP {status}: {body[:200]!r}")

    def setUp(self):
        self.references = []
        self.addCleanup(self.cleanup_references)

    def upload(self, filename, contents, authenticated=True):
        headers = {"Content-Type": f"multipart/form-data; boundary={BOUNDARY}"}
        if authenticated:
            headers.update(self.auth)
        return request("/api/v4/file", "POST", multipart(filename, contents), headers)

    def upload_pdf(self, filename):
        contents = f"%PDF-1.7\n{uuid.uuid4()}\n".encode()
        status, body, _ = self.upload(filename, contents)
        self.assertEqual(status, 200, body[:300])
        reference = json.loads(body)
        self.references.append(reference["public_id"])
        self.assertEqual(reference["content_type"], "application/pdf")
        self.assertEqual(reference["size_bytes"], len(contents))
        return reference, contents

    def cleanup_references(self):
        for public_id in self.references:
            request(f"/api/v4/file/{public_id}", "DELETE", headers=self.auth)

    def test_upload_dedup_list_safe_download_and_tenant_scope(self):
        status, _, _ = self.upload("unauthenticated.pdf", b"%PDF-1.7\n", False)
        self.assertEqual(status, 401)

        first, contents = self.upload_pdf("first.pdf")
        status, body, _ = self.upload("second-name.pdf", contents)
        self.assertEqual(status, 200, body[:300])
        second = json.loads(body)
        self.references.append(second["public_id"])
        self.assertNotEqual(first["public_id"], second["public_id"])

        status, body, _ = request("/api/v4/file/list", headers=self.auth)
        self.assertEqual(status, 200)
        listed = {file["public_id"] for file in json.loads(body)["files"]}
        self.assertTrue({first["public_id"], second["public_id"]} <= listed)

        file_path = urlsplit(first["url"]).path
        status, body, headers = request(file_path)
        self.assertEqual((status, body), (200, contents))
        self.assertTrue(headers.get("Content-Disposition", "").startswith("attachment; filename*=UTF-8''"))
        self.assertEqual(headers.get("X-Content-Type-Options"), "nosniff")
        self.assertEqual(headers.get("Content-Length"), str(len(contents)))

        status, body, _ = request(file_path, host="unknown-tenant.example")
        self.assertEqual(status, 404, body[:200])
        status, body, _ = request(file_path, host=self.second_tenant_hostname)
        self.assertEqual(status, 404, body[:200])

        status, _, _ = request(f"/api/v4/file/{first['public_id']}", "DELETE", headers=self.auth)
        self.assertEqual(status, 200)
        status, body, _ = request(urlsplit(second["url"]).path)
        self.assertEqual((status, body), (200, contents))
        status, _, _ = request(f"/api/v4/file/{second['public_id']}", "DELETE", headers=self.auth)
        self.assertEqual(status, 200)
        status, _, _ = request(file_path)
        self.assertEqual(status, 404)

    def test_mime_image_and_size_restrictions(self):
        status, body, _ = self.upload("plain.txt", b"plain text")
        self.assertIn(status, (400, 403), body[:200])

        png = bytes.fromhex(
            "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c489"
            "0000000b49444154789c636000020000050001a5f645400000000049454e44ae426082"
        )
        status, body, _ = self.upload("misnamed.pdf", png)
        self.assertIn(status, (400, 403), body[:200])

        self.edit_site({"file_max_upload_size_bytes": 10})
        status, body, _ = self.upload("oversized.pdf", b"%PDF-1.7\nmore than ten bytes")
        self.assertEqual(status, 413, body[:200])
        self.edit_site({"file_max_upload_size_bytes": 1048576})

    def test_private_site_requires_authentication_for_download(self):
        reference, _ = self.upload_pdf("private.pdf")
        self.edit_site({"private_instance": True})
        self.addCleanup(self.edit_site, {"private_instance": False})

        status, _, _ = request(urlsplit(reference["url"]).path)
        self.assertEqual(status, 401)


if __name__ == "__main__":
    unittest.main(verbosity=2)
