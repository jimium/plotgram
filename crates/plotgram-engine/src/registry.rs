//! Default algorithm registry.

use std::collections::BTreeMap;
use std::sync::Arc;

use plotgram_engine_api::{EdgeRouter, LayoutAlgorithm};

use crate::layout::HierarchicalLayout;
use plotgram_router::{
    CurvedEdgeRouter, OctilinearEdgeRouter, OrthogonalEdgeRouter, PolylineEdgeRouter,
    StraightEdgeRouter,
};

pub struct Registry {
    layouts: BTreeMap<&'static str, Arc<dyn LayoutAlgorithm>>,
    routers: BTreeMap<&'static str, Arc<dyn EdgeRouter>>,
}

impl Registry {
    pub fn standard() -> Self {
        let mut layouts: BTreeMap<&'static str, Arc<dyn LayoutAlgorithm>> = BTreeMap::new();
        let hier = Arc::new(HierarchicalLayout);
        layouts.insert(hier.name(), hier);

        let mut routers: BTreeMap<&'static str, Arc<dyn EdgeRouter>> = BTreeMap::new();
        for router in [
            Arc::new(OrthogonalEdgeRouter) as Arc<dyn EdgeRouter>,
            Arc::new(StraightEdgeRouter),
            Arc::new(PolylineEdgeRouter),
            Arc::new(OctilinearEdgeRouter),
            Arc::new(CurvedEdgeRouter),
        ] {
            routers.insert(router.name(), router);
        }

        Self { layouts, routers }
    }

    pub fn layout(&self, name: &str) -> Option<Arc<dyn LayoutAlgorithm>> {
        self.layouts.get(name).cloned()
    }

    pub fn router(&self, name: &str) -> Option<Arc<dyn EdgeRouter>> {
        self.routers.get(name).cloned()
    }
}
