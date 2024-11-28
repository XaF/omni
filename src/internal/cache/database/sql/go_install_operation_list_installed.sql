-- List all the installed go tools
SELECT
    import_path,
    version
FROM
    go_installed;
