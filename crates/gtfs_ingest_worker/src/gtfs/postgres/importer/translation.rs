use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TranslationKind {
    Agency,
    Stop,
    Route,
    Trip,
    Service,
    Shape,
}

#[derive(Debug, Default)]
pub struct TranslationMaps {
    next_item_id: i32,
    agency: HashMap<String, i32>,
    stop: HashMap<String, i32>,
    route: HashMap<String, i32>,
    trip: HashMap<String, i32>,
    service: HashMap<String, i32>,
    shape: HashMap<String, i32>,
}

impl TranslationMaps {
    pub fn new() -> Self {
        Self {
            next_item_id: 1,
            ..Self::default()
        }
    }

    pub fn allocate_item_id(&mut self) -> i32 {
        let item_id = self.next_item_id;
        self.next_item_id += 1;
        item_id
    }

    pub fn get_or_insert(&mut self, kind: TranslationKind, gtfs_id: &str) -> i32 {
        if let Some(item_id) = self.map(kind).get(gtfs_id) {
            return *item_id;
        }

        let item_id = self.allocate_item_id();
        self.map(kind).insert(gtfs_id.to_owned(), item_id);
        item_id
    }

    pub fn optional_reference(
        &mut self,
        kind: TranslationKind,
        gtfs_id: Option<&str>,
    ) -> Option<i32> {
        let gtfs_id = gtfs_id.filter(|value| !value.is_empty())?;
        Some(self.get_or_insert(kind, gtfs_id))
    }

    fn map(&mut self, kind: TranslationKind) -> &mut HashMap<String, i32> {
        match kind {
            TranslationKind::Agency => &mut self.agency,
            TranslationKind::Stop => &mut self.stop,
            TranslationKind::Route => &mut self.route,
            TranslationKind::Trip => &mut self.trip,
            TranslationKind::Service => &mut self.service,
            TranslationKind::Shape => &mut self.shape,
        }
    }
}
