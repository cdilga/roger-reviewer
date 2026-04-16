ALTER TABLE outbound_draft_items
ADD COLUMN target_locator TEXT NOT NULL DEFAULT '';

ALTER TABLE outbound_draft_items
ADD COLUMN body TEXT NOT NULL DEFAULT '';

UPDATE outbound_draft_items
SET
    target_locator = COALESCE((
        SELECT od.target_locator
        FROM outbound_drafts od
        WHERE od.id = outbound_draft_items.id
    ), target_locator),
    body = COALESCE((
        SELECT od.body
        FROM outbound_drafts od
        WHERE od.id = outbound_draft_items.id
    ), body)
WHERE target_locator = '' OR body = '';
