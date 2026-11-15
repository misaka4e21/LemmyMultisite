# Purge dependency inventory

PostgreSQL foreign keys in the temporary migrated database show the following deletion paths. The checks must run in the same serialized transaction as deletion; a check followed by an unlocked delete permits another site to add a dependency in between.

| Target | Data reached by deletion or cleanup | Protected-dependency checks |
| --- | --- | --- |
| Person | The current handler keeps the Person row but asynchronously clears all creator posts and comments, removes all community moderator relationships, invalidates credentials, clears profile data and removes uploads. It also queues a site ban activity. | Check person presence, creator content and its presence, moderator relationships, media uses and federation recipients. The worker must recheck ownership and use site-scoped mutations while holding the relevant locks. A global account cleanup cannot be treated as a site-scoped operation. |
| Community | Foreign keys cascade to posts, comments, community actions, reports, notifications, moderation logs, language, tags, multi-community entries and `community_site`; descendant posts cascade further. | Check other-site and remote descendants, site presence, users' actions, report ownership and state, notifications, moderation history, multi-community entries and media references before deleting the community and its descendants. |
| Post | Foreign keys cascade to comments, post actions, reports, notifications, moderation logs, saved/liked/combined rows, thumbnail sources, `post_site` and `media_reference`; comments cascade further. | Check other-site and remote comments, site presence, actions and saved state, report ownership and state, notifications, moderation history and media references. A reference linked through `media_reference.post_id` can belong to another site. |
| Comment | Foreign keys cascade to comment actions, reports, notifications, moderation logs, saved/liked/combined rows and `comment_site`. | Check other-site presence, actions and saved state, report ownership and state, notifications, moderation history and content media references. Comment paths also need examination for descendant relationships. |

The purge handlers submit federation activities and write moderation logs after the primary deletion, except person purge, which submits an activity and starts background work before its ban and log writes. Rejections must occur before all of these side effects. The person worker must not rely solely on a check performed by the request handler.

The foreign key inventory was obtained from `pg_constraint` in a disposable migrated PostgreSQL database. Derived tables and triggers must be checked again when implementing each target because their own cascades can extend these paths.

## Caller audit

`purge_person` verifies the administrator's site, reads the target through `read_owned_by_site`, and passes that site into the background worker. The worker repeats the ownership read before calling `purge_user_account`. That helper now deletes only comments, posts, uploaded image references, and community moderator roles owned by the site, and reads the site's local login when invalidating credentials. `Person::delete_account` also limits local-user email cleanup to the supplied site.

The self-service deletion handler in `api_crud/user/delete.rs` passes the local user's site. Federated deletion passes the tenant from `context.site_view()`. Both callers share `Person::delete_account`, whose local-user lookup is site-filtered. The focused test `delete_account_clears_only_the_site_bound_login` covers that helper.

Site bans with `remove_or_restore_data` pass `Some(local_site_id)` from `ban_person`; the cleanup then scopes posts, comments, private messages, votes, top-moderator communities, and image uploads to that site. AP block and undo-block activities pass `None` because they represent the authorized origin-instance action and keep the existing installation-wide behavior. The site-scoped ban test verifies that another site's owned post and comment stay visible, while the same-site objects are marked removed.

Rejected purge paths still return before modlog, activity, or background task creation. The existing `purge_guard` test covers foreign descendants, foreign presence, saved state, reports, media references, and concurrent dependency insertion.
