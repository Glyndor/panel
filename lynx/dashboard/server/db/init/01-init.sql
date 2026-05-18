-- Creates isolated app user with minimal privileges.
-- Runs once on first PostgreSQL container startup via /docker-entrypoint-initdb.d/.
-- LYNX_APP_PASS is substituted by the init wrapper using the mounted secret.

\set app_pass `cat /run/secrets/lynx-dashboard-pg-pass`

CREATE USER lynx_dashboard_app WITH PASSWORD :'app_pass' NOSUPERUSER NOCREATEDB NOCREATEROLE;

GRANT CONNECT ON DATABASE lynx_dashboard TO lynx_dashboard_app;

\connect lynx_dashboard

GRANT USAGE ON SCHEMA public TO lynx_dashboard_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO lynx_dashboard_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT USAGE, SELECT ON SEQUENCES TO lynx_dashboard_app;
