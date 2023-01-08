-- Add migration script here
ALTER TABLE message
ALTER COLUMN num DROP DEFAULT;

DROP SEQUENCE message_num_seq;

ALTER TABLE message
ALTER COLUMN num TYPE int;