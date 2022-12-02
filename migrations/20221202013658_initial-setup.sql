-- Add migration script here
CREATE TABLE config (
    guild_id bigint PRIMARY KEY,
    host_role_id bigint
);
CREATE TABLE channel_message (
    channel_id bigint PRIMARY KEY,
    message_id bigint NOT NULL
);
CREATE TABLE react (
    channel_id bigint NOT NULL,
    user_id bigint NOT NULL,
    PRIMARY KEY (channel_id, user_id),
    CONSTRAINT fk_reacts FOREIGN KEY (channel_id) REFERENCES channel_message(channel_id) ON DELETE CASCADE
);