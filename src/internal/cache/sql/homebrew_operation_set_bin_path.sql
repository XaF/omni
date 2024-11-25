-- Set the bin path for Homebrew itself (the generic one)
-- :param1: The path to the binary directory where most brew-installed executables are located
INSERT INTO metadata (
    key,
    value
)
VALUES (
    'homebrew.bin_path',
    ?1
) ON CONFLICT (key) DO UPDATE
SET
    value = ?1
WHERE key = 'homebrew.bin_path';
