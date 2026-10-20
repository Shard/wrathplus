//! The page-text query pair, the ask-once cache behind every readable book: a `PageText.wdb` id
//! names one page, and each answer carries the next page's id (`0` = last). The first id comes
//! from an item template's `PageText` or a `GAMEOBJECT_TYPE_TEXT` object's `data[0]`; mail bodies
//! use `CMSG_ITEM_TEXT_QUERY` instead.

/// Body of `CMSG_PAGE_TEXT_QUERY` (`0x005A`): `u32 pageId`, then the asking object's `u64 guid`,
/// which the 1.12 client appends (`0x564730`, at `0x56485d`) and vmangos reads only if present,
/// then discards (`QueryPageText::ReadFromWorldPacket`).
pub fn page_text_query(page_id: u32, guid: u64) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&page_id.to_le_bytes());
    body.extend_from_slice(&guid.to_le_bytes());
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_body_is_page_id_then_guid() {
        assert_eq!(
            page_text_query(333, 0xF110_0000_0000_0042),
            [
                0x4D, 0x01, 0x00, 0x00, // pageId 333
                0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0xF1, // guid
            ]
        );
    }
}
