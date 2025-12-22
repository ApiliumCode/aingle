SELECT
    hash
FROM
    SGdOp
WHERE
    SgdOp.authored_timestamp_ms >= :from
    AND SgdOp.authored_timestamp_ms < :to
    AND SgdOp.when_integrated IS NOT NULL