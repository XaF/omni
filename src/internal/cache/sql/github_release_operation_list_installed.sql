-- List all the installed github releases
SELECT
    repository,
    version
FROM
    github_release_installed;
