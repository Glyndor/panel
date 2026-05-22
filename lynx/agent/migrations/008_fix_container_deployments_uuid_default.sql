-- Fix UUID default for container_deployments — original migration 007 used
-- gen_random_uuid() (UUID v4) which violates the project-wide UUID v7 rule.
-- PostgreSQL 18 provides uuidv7() natively.
ALTER TABLE container_deployments ALTER COLUMN id SET DEFAULT uuidv7();
