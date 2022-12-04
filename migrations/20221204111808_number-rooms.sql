-- Add migration script here
ALTER TABLE message
ADD COLUMN num SERIAL NOT NULL;