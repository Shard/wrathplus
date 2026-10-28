use super::*;
impl Transport {
    pub(crate) fn test_deck() -> Self {
        let node = |i, x| wrathplus_formats::TaxiPathNode {
            id: i,
            path_id: 1,
            node_index: i,
            map_id: 0,
            pos: [x, 0.0, 0.0],
            flags: 0,
            delay: 0,
        };
        let path = [node(0, 0.0), node(1, 50.0), node(2, 100.0), node(3, 150.0)];
        Self {
            drive: Drive::Taxi(Arc::new(
                TransportTimetable::build(&path, 30.0, 1.0).unwrap(),
            )),
            was_moving: false,
        }
    }
}
