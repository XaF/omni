-- List all the installed cargo tools
SELECT
    crate,
    version
FROM
    cargo_installed;
