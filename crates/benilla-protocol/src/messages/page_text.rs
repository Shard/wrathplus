//! The page-text query pair, the ask-once cache behind every readable book: a `PageText.wdb` id
//! names one page, and each answer carries the next page's id (`0` = last). The first id comes
//! from an item template's `PageText` or a `GAMEOBJECT_TYPE_TEXT` object's `data[0]`; mail bodies
//! use `CMSG_ITEM_TEXT_QUERY` instead.
