-- Get the bin path for Homebrew itself (the generic one)
-- :return: The path to the binary directory where most brew-installed executables are located
SELECT
    value AS bin_path
FROM
    metadata
WHERE
    key = 'homebrew.bin_path';
