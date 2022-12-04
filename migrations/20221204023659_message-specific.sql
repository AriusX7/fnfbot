-- Add migration script here
CREATE TABLE message (message_id bigint PRIMARY KEY);

INSERT INTO message
SELECT message_id
FROM channel_message;

ALTER TABLE react
ADD COLUMN message_id bigint CONSTRAINT signups_message_id_fk REFERENCES message(message_id) ON DELETE CASCADE;

UPDATE react
SET message_id = cm.message_id
FROM channel_message cm
WHERE react.channel_id = cm.channel_id;

ALTER TABLE react DROP CONSTRAINT react_pkey;

ALTER TABLE react DROP COLUMN channel_id;

ALTER TABLE react
ADD PRIMARY KEY (message_id, user_id);

ALTER TABLE react
    RENAME TO signup;

DROP TABLE channel_message;