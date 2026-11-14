## Why

Registration approval can modify another site's application before checking its ownership. Site administrator image deletion can purge shared media, and permanent deletion can affect other sites through shared rows, cascades, and background cleanup.

## What Changes

- Authorize registration decisions against the request site before any update or email.
- Restrict site administrator media deletion to the current site's references and protect shared aliases during concurrent operations.
- Protect other sites' content, state, and media when purging an owned person, community, post, or comment, including asynchronous and federated side effects.
- Add two-site regression tests for authorized actions, rejected targets, shared dependencies, and failure paths.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `multisite/site-administration`: Define the authorization and side-effect boundary for registration decisions.
- `multisite/media-storage`: Define site administrator deletion and concurrent reference protection.
- `multisite/activitypub-objects`: Define safe behavior when permanent deletion encounters shared or cross-site dependencies.

## Impact

Changes affect registration handlers and database queries, image deletion and media reference lifecycle, purge handlers and background cleanup, and their database and API tests. Existing API shapes remain; requests that previously crossed tenant boundaries will be rejected or constrained to the current site.
