UPDATE
    SgdOp
SET
    when_integrated = :when_integrated,
    when_integrated_ns = :when_integrated_ns,
    validation_stage = NULL
WHERE
    validation_stage = 3
    AND validation_status IS NOT NULL
    AND CASE
        SgdOp.type
        WHEN :store_entry THEN 1
        WHEN :store_element THEN 1
        WHEN :register_activity THEN SgdOp.header_hash IN (
            SELECT
                Header.hash
            FROM
                Header
            WHERE
                Header.prev_hash IN (
                    SELECT
                        OP_DEP.header_hash
                    FROM
                        SgdOp AS OP_DEP
                    WHERE
                        OP_DEP.when_integrated IS NOT NULL
                        AND OP_DEP.type = :register_activity
                )
                OR Header.prev_hash IS NULL
        )
        WHEN :updated_content THEN SgdOp.header_hash IN (
            SELECT
                Header.hash
            FROM
                Header
            WHERE
                Header.original_header_hash IN (
                    SELECT
                        OP_DEP.header_hash
                    FROM
                        SgdOp AS OP_DEP
                    WHERE
                        OP_DEP.when_integrated IS NOT NULL
                        AND OP_DEP.type = :store_entry
                )
        )
        WHEN :updated_element THEN SgdOp.header_hash IN (
            SELECT
                Header.hash
            FROM
                Header
            WHERE
                Header.original_header_hash IN (
                    SELECT
                        OP_DEP.header_hash
                    FROM
                        SgdOp AS OP_DEP
                    WHERE
                        OP_DEP.when_integrated IS NOT NULL
                        AND OP_DEP.type = :store_element
                )
        )
        WHEN :deleted_by THEN SgdOp.header_hash IN (
            SELECT
                Header.hash
            FROM
                Header
            WHERE
                Header.deletes_header_hash IN (
                    SELECT
                        OP_DEP.header_hash
                    FROM
                        SgdOp AS OP_DEP
                    WHERE
                        OP_DEP.when_integrated IS NOT NULL
                        AND OP_DEP.type = :store_element
                )
        )
        WHEN :deleted_entry_header THEN SgdOp.header_hash IN (
            SELECT
                Header.hash
            FROM
                Header
            WHERE
                Header.deletes_header_hash IN (
                    SELECT
                        OP_DEP.header_hash
                    FROM
                        SgdOp AS OP_DEP
                    WHERE
                        OP_DEP.when_integrated IS NOT NULL
                        AND OP_DEP.type = :store_entry
                )
        )
        WHEN :create_link THEN SgdOp.header_hash IN (
            SELECT
                Header.hash
            FROM
                Header
            WHERE
                Header.base_hash IN (
                    SELECT
                        H_DEP.entry_hash
                    FROM
                        Header AS H_DEP
                        JOIN SgdOp AS OP_DEP ON OP_DEP.header_hash = H_DEP.hash
                    WHERE
                        OP_DEP.when_integrated IS NOT NULL
                        AND OP_DEP.type = :store_entry
                )
                OR Header.prev_hash IS NULL
        )
        WHEN :delete_link THEN SgdOp.header_hash IN (
            SELECT
                Header.hash
            FROM
                Header
            WHERE
                Header.create_link_hash IN (
                    SELECT
                        OP_DEP.header_hash
                    FROM
                        SgdOp AS OP_DEP
                    WHERE
                        OP_DEP.when_integrated IS NOT NULL
                        AND OP_DEP.type = :create_link
                )
        )
    END 