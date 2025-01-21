-- Start a transaction
BEGIN TRANSACTION;

-- Clear the cached github releases
DELETE FROM github_releases;

-- Update the user_version to 3
PRAGMA user_version = 3;

-- Commit the transaction
COMMIT;
