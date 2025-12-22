SELECT
    hash
FROM
    SGdOp
WHERE
    SgdOp.authored_timestamp_ms >= :from
    AND SgdOp.authored_timestamp_ms < :to
    AND storage_center_loc >= :storage_start_1
    AND storage_center_loc <= :storage_end_1 
    AND SgdOp.when_integrated IS NOT NULL