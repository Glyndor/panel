-- Fix UUID column default: gen_random_uuid() generates v4; project requires v7.
-- PostgreSQL 18 provides uuidv7() built-in.
ALTER TABLE nginx_configs ALTER COLUMN id SET DEFAULT uuidv7();
