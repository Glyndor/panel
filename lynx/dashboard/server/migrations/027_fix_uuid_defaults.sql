-- Fix UUID column defaults: gen_random_uuid() generates v4; project requires v7.
-- PostgreSQL 18 provides uuidv7() built-in.
ALTER TABLE security_alerts ALTER COLUMN id SET DEFAULT uuidv7();
