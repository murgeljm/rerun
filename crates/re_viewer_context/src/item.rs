use re_entity_db::InstancePath;
use re_log_types::{ComponentPath, DataPath, EntityPath};

use crate::{ContainerId, SpaceViewId};

/// One "thing" in the UI.
///
/// This is the granularity of what is selectable and hoverable.
#[derive(Clone, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
pub enum Item {
    /// A recording (or blueprint)
    StoreId(re_log_types::StoreId),

    /// A component of an entity from the data store.
    ComponentPath(ComponentPath),

    /// A space view.
    SpaceView(SpaceViewId),

    /// An entity or instance from the data store.
    InstancePath(InstancePath),

    /// An entity or instance in the context of a space view's data results.
    DataResult(SpaceViewId, InstancePath),

    /// A container.
    Container(ContainerId),
}

impl Item {
    pub fn entity_path(&self) -> Option<&EntityPath> {
        match self {
            Item::ComponentPath(component_path) => Some(&component_path.entity_path),
            Item::SpaceView(_) | Item::Container(_) | Item::StoreId(_) => None,
            Item::InstancePath(instance_path) | Item::DataResult(_, instance_path) => {
                Some(&instance_path.entity_path)
            }
        }
    }
}

impl From<SpaceViewId> for Item {
    #[inline]
    fn from(space_view_id: SpaceViewId) -> Self {
        Self::SpaceView(space_view_id)
    }
}

impl From<ComponentPath> for Item {
    #[inline]
    fn from(component_path: ComponentPath) -> Self {
        Self::ComponentPath(component_path)
    }
}

impl From<InstancePath> for Item {
    #[inline]
    fn from(instance_path: InstancePath) -> Self {
        Self::InstancePath(instance_path)
    }
}

impl std::str::FromStr for Item {
    type Err = re_log_types::PathParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let DataPath {
            entity_path,
            instance_key,
            component_name,
        } = DataPath::from_str(s)?;

        match (instance_key, component_name) {
            (Some(instance_key), Some(_component_name)) => {
                // TODO(emilk): support selecting a specific component of a specific instance.
                Err(re_log_types::PathParseError::UnexpectedInstanceKey(
                    instance_key,
                ))
            }
            (Some(instance_key), None) => Ok(Item::InstancePath(InstancePath::instance(
                entity_path,
                instance_key,
            ))),
            (None, Some(component_name)) => Ok(Item::ComponentPath(ComponentPath {
                entity_path,
                component_name,
            })),
            (None, None) => Ok(Item::InstancePath(InstancePath::entity_splat(entity_path))),
        }
    }
}

impl std::fmt::Debug for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Item::StoreId(store_id) => store_id.fmt(f),
            Item::ComponentPath(s) => s.fmt(f),
            Item::SpaceView(s) => write!(f, "{s:?}"),
            Item::InstancePath(path) => write!(f, "{path}"),
            Item::DataResult(space_view_id, instance_path) => {
                write!(f, "({space_view_id:?}, {instance_path}")
            }
            Item::Container(tile_id) => write!(f, "(tile: {tile_id:?})"),
        }
    }
}

impl Item {
    pub fn kind(self: &Item) -> &'static str {
        match self {
            Item::StoreId(store_id) => match store_id.kind {
                re_log_types::StoreKind::Recording => "Recording ID",
                re_log_types::StoreKind::Blueprint => "Blueprint ID",
            },
            Item::InstancePath(instance_path) => {
                if instance_path.instance_key.is_specific() {
                    "Entity Instance"
                } else {
                    "Entity"
                }
            }
            Item::ComponentPath(_) => "Entity Component",
            Item::SpaceView(_) => "Space View",
            Item::Container(_) => "Container",
            Item::DataResult(_, instance_path) => {
                if instance_path.instance_key.is_specific() {
                    "Data Result Instance"
                } else {
                    "Data Result Entity"
                }
            }
        }
    }
}

/// If the given item refers to the first element of an instance with a single element, resolve to a splatted entity path.
pub fn resolve_mono_instance_path_item(
    query: &re_data_store::LatestAtQuery,
    store: &re_data_store::DataStore,
    item: &Item,
) -> Item {
    // Resolve to entity path if there's only a single instance.
    match item {
        Item::InstancePath(instance_path) => {
            Item::InstancePath(resolve_mono_instance_path(query, store, instance_path))
        }
        Item::DataResult(space_view_id, instance_path) => Item::DataResult(
            *space_view_id,
            resolve_mono_instance_path(query, store, instance_path),
        ),
        Item::StoreId(_) | Item::ComponentPath(_) | Item::SpaceView(_) | Item::Container(_) => {
            item.clone()
        }
    }
}

/// If the given path refers to the first element of an instance with a single element, resolve to a splatted entity path.
pub fn resolve_mono_instance_path(
    query: &re_data_store::LatestAtQuery,
    store: &re_data_store::DataStore,
    instance: &re_entity_db::InstancePath,
) -> re_entity_db::InstancePath {
    re_tracing::profile_function!();

    if instance.instance_key.0 == 0 {
        let Some(components) = store.all_components(&query.timeline, &instance.entity_path) else {
            // No components at all, return splatted entity.
            return re_entity_db::InstancePath::entity_splat(instance.entity_path.clone());
        };
        for component in components {
            if let Some((_, _row_id, instances)) = re_query::get_component_with_instances(
                store,
                query,
                &instance.entity_path,
                component,
            ) {
                if instances.len() > 1 {
                    return instance.clone();
                }
            }
        }

        // All instances had only a single element or less, resolve to splatted entity.
        return re_entity_db::InstancePath::entity_splat(instance.entity_path.clone());
    }

    instance.clone()
}
