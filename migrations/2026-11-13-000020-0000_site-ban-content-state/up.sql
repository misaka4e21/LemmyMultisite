ALTER TABLE post_site ADD COLUMN ban_removed boolean NOT NULL DEFAULT false;
ALTER TABLE comment_site ADD COLUMN ban_removed boolean NOT NULL DEFAULT false;
ALTER TABLE private_message_site ADD COLUMN ban_removed boolean NOT NULL DEFAULT false;
