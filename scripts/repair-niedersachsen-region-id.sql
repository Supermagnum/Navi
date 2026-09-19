-- Repair place_index.db rows wrongly tagged europe/norway/niedersachsen.
-- Correct pack-catalog region id: europe/germany/niedersachsen
-- (https://navigate-me.duckdns.org/ current.json chips under europe/germany).
--
-- DO NOT apply this to a live device DB without the procedure below.
-- Rehearse on a /tmp copy first and record before/after counts.
--
-- Procedure (tablet / production data):
-- 1. adb shell am force-stop no.navi.app
-- 2. Backup on host:
--      adb pull .../files/place_index.db /tmp/place_index.db.bak
--      adb pull .../files/place_index.db-wal /tmp/  (if present)
--      adb pull .../files/place_index.db-shm /tmp/  (if present)
--      adb pull .../files/place-index-ready.json /tmp/
-- 3. On the working copy: PRAGMA wal_checkpoint(TRUNCATE);
-- 4. Apply this script (sqlite3 or Python sqlite3).
-- 5. Fix place-index-ready.json: remove "europe/norway/niedersachsen";
--    ensure "europe/germany/niedersachsen" is present if that region is indexed.
-- 6. Push repaired files only after verifying counts on the copy.
--
-- Verification queries (before / after):
--   SELECT region_id, COUNT(*) FROM name_entries
--     WHERE region_id LIKE '%niedersachsen%' GROUP BY 1;
--   SELECT * FROM name_index_build WHERE region_id LIKE '%niedersachsen%';

BEGIN;

UPDATE name_entries
SET region_id = 'europe/germany/niedersachsen'
WHERE region_id = 'europe/norway/niedersachsen';

DELETE FROM name_index_build
WHERE region_id = 'europe/norway/niedersachsen';

INSERT INTO name_index_build (region_id, complete)
VALUES ('europe/germany/niedersachsen', 1)
ON CONFLICT(region_id) DO UPDATE SET complete = 1;

COMMIT;
