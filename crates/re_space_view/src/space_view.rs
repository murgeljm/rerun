use itertools::{FoldWhile, Itertools};
use re_data_store::LatestAtQuery;
use re_entity_db::{EntityDb, EntityPath, EntityProperties, VisibleHistory};
use re_entity_db::{EntityPropertiesComponent, EntityPropertyMap};

use crate::SpaceViewContents;
use re_log_types::{DataRow, EntityPathFilter, RowId};
use re_query::query_archetype;
use re_types::blueprint::archetypes as blueprint_archetypes;
use re_types::{
    blueprint::components::{SpaceViewOrigin, Visible},
    components::Name,
};
use re_types_core::archetypes::Clear;
use re_types_core::Archetype as _;
use re_viewer_context::{
    blueprint_timepoint_for_writes, DataResult, DynSpaceViewClass, PerSystemEntities,
    PropertyOverrides, SpaceViewClassIdentifier, SpaceViewId, SpaceViewState, StoreContext,
    SystemCommand, SystemCommandSender as _, SystemExecutionOutput, ViewQuery, ViewerContext,
};

// ----------------------------------------------------------------------------

/// The name of a space view.
///
/// Space views are unnamed by default, but have a placeholder name to be used in the UI.
#[derive(Clone, Debug)]
pub enum SpaceViewName {
    /// This space view has been given a name by the user.
    Named(String),

    /// This space view is unnamed and should be displayed with this placeholder name.
    Placeholder(String),
}

impl AsRef<str> for SpaceViewName {
    #[inline]
    fn as_ref(&self) -> &str {
        match self {
            SpaceViewName::Named(name) | SpaceViewName::Placeholder(name) => name,
        }
    }
}

// ----------------------------------------------------------------------------

/// A view of a space.
///
/// Note: [`SpaceViewBlueprint`] doesn't implement Clone because it stores an internal
/// uuid used for identifying the path of its data in the blueprint store. It's ambiguous
/// whether the intent is for a clone to write to the same place.
///
/// If you want a new space view otherwise identical to an existing one, use
/// `re_viewport::ViewportBlueprint::duplicate_space_view`.
pub struct SpaceViewBlueprint {
    pub id: SpaceViewId,
    pub display_name: Option<String>,
    class_identifier: SpaceViewClassIdentifier,

    /// The "anchor point" of this space view.
    /// The transform at this path forms the reference point for all scene->world transforms in this space view.
    /// I.e. the position of this entity path in space forms the origin of the coordinate system in this space view.
    /// Furthermore, this is the primary indicator for heuristics on what entities we show in this space view.
    pub space_origin: EntityPath,

    /// The content of this space view as defined by its queries.
    pub contents: SpaceViewContents,

    /// True if this space view is visible in the UI.
    pub visible: bool,

    /// Pending blueprint writes for nested components from duplicate.
    pending_writes: Vec<DataRow>,
}

impl SpaceViewBlueprint {
    /// Creates a new [`SpaceViewBlueprint`] with a single [`SpaceViewContents`].
    ///
    /// This [`SpaceViewBlueprint`] is ephemeral. If you want to make it permanent you
    /// must call [`Self::save_to_blueprint_store`].
    pub fn new(
        space_view_class: SpaceViewClassIdentifier,
        space_path: &EntityPath,
        content: EntityPathFilter,
    ) -> Self {
        let id = SpaceViewId::random();

        Self {
            display_name: None,
            class_identifier: space_view_class,
            id,
            space_origin: space_path.clone(),
            contents: SpaceViewContents::new(id, space_view_class, content),
            visible: true,
            pending_writes: Default::default(),
        }
    }

    /// Placeholder name displayed in the UI if the user hasn't explicitly named the space view.
    #[allow(clippy::unused_self)]
    pub fn missing_name_placeholder(&self) -> String {
        let entity_path = self
            .space_origin
            .iter()
            .rev()
            .fold_while(String::new(), |acc, path| {
                if acc.len() > 10 {
                    FoldWhile::Done(format!("…/{acc}"))
                } else {
                    FoldWhile::Continue(format!(
                        "{}{}{}",
                        path.ui_string(),
                        if acc.is_empty() { "" } else { "/" },
                        acc
                    ))
                }
            })
            .into_inner();

        if entity_path.is_empty() {
            "/".to_owned()
        } else {
            entity_path
        }
    }

    /// Returns this space view's display name
    ///
    /// When returning [`SpaceViewName::Placeholder`], the UI should display the resulting name using
    /// `re_ui::LabelStyle::Unnamed`.
    pub fn display_name_or_default(&self) -> SpaceViewName {
        self.display_name.clone().map_or_else(
            || SpaceViewName::Placeholder(self.missing_name_placeholder()),
            SpaceViewName::Named,
        )
    }

    /// Attempt to load a [`SpaceViewBlueprint`] from the blueprint store.
    pub fn try_from_db(
        id: SpaceViewId,
        blueprint_db: &EntityDb,
        query: &LatestAtQuery,
    ) -> Option<Self> {
        re_tracing::profile_function!();

        let blueprint_archetypes::SpaceViewBlueprint {
            display_name,
            class_identifier,
            space_origin,
            visible,
        } = query_archetype(blueprint_db.store(), query, &id.as_entity_path())
            .and_then(|arch| arch.to_archetype())
            .map_err(|err| {
                if !matches!(err, re_query::QueryError::PrimaryNotFound(_)) {
                    if cfg!(debug_assertions) {
                        re_log::error!("Failed to load SpaceView blueprint: {err}.");
                    } else {
                        re_log::debug!("Failed to load SpaceView blueprint: {err}.");
                    }
                }
            })
            .ok()?;

        let space_origin = space_origin.map_or_else(EntityPath::root, |origin| origin.0.into());
        let class_identifier: SpaceViewClassIdentifier = class_identifier.0.as_str().into();
        let display_name = display_name.map(|v| v.0.to_string());

        let content =
            SpaceViewContents::from_db_or_default(id, blueprint_db, query, class_identifier);
        let visible = visible.map_or(true, |v| v.0);

        Some(Self {
            id,
            display_name,
            class_identifier,
            space_origin,
            contents: content,
            visible,
            pending_writes: Default::default(),
        })
    }

    /// Persist the entire [`SpaceViewBlueprint`] to the blueprint store.
    ///
    /// This only needs to be called if the [`SpaceViewBlueprint`] was created with [`Self::new`].
    ///
    /// Otherwise, incremental calls to `set_` functions will write just the necessary component
    /// update directly to the store.
    pub fn save_to_blueprint_store(&self, ctx: &ViewerContext<'_>) {
        let timepoint = blueprint_timepoint_for_writes();

        let Self {
            id,
            display_name,
            class_identifier,
            space_origin,
            contents,
            visible,
            pending_writes,
        } = self;

        let mut arch = blueprint_archetypes::SpaceViewBlueprint::new(class_identifier.as_str())
            .with_space_origin(space_origin)
            .with_visible(*visible);

        if let Some(display_name) = display_name {
            arch = arch.with_display_name(display_name.clone());
        }

        // Start with the pending writes, which explicitly filtered out the `SpaceViewBlueprint`
        // components from the top level.
        let mut deltas = pending_writes.clone();

        // Add all the additional components from the archetype
        if let Ok(row) =
            DataRow::from_archetype(RowId::new(), timepoint.clone(), id.as_entity_path(), &arch)
        {
            deltas.push(row);
        }

        contents.save_to_blueprint_store(ctx);

        ctx.command_sender
            .send_system(SystemCommand::UpdateBlueprint(
                ctx.store_context.blueprint.store_id().clone(),
                deltas,
            ));
    }

    /// Creates a new [`SpaceViewBlueprint`] with the same contents, but a different [`SpaceViewId`]
    ///
    /// Also duplicates all the queries in the space view.
    pub fn duplicate(&self, blueprint: &EntityDb, query: &LatestAtQuery) -> Self {
        let mut pending_writes = Vec::new();

        let current_path = self.entity_path();
        let new_id = SpaceViewId::random();
        let new_path = new_id.as_entity_path();

        // Create pending write operations to duplicate the entire subtree
        // TODO(jleibs): This should be a helper somewhere.
        if let Some(tree) = blueprint.tree().subtree(&current_path) {
            tree.visit_children_recursively(&mut |path, info| {
                let sub_path: EntityPath = new_path
                    .iter()
                    .chain(&path[current_path.len()..])
                    .cloned()
                    .collect();

                if let Ok(row) = DataRow::from_cells(
                    RowId::new(),
                    blueprint_timepoint_for_writes(),
                    sub_path,
                    1,
                    info.components
                        .keys()
                        // It's important that we don't include the SpaceViewBlueprint's components
                        // since those will be updated separately and may contain different data.
                        .filter(|component| {
                            *path != current_path
                                || !blueprint_archetypes::SpaceViewBlueprint::all_components()
                                    .contains(component)
                        })
                        .filter_map(|component| {
                            blueprint
                                .store()
                                .latest_at(query, path, *component, &[*component])
                                .and_then(|(_, _, cells)| cells[0].clone())
                        }),
                ) {
                    if row.num_cells() > 0 {
                        pending_writes.push(row);
                    }
                }
            });
        }

        // SpaceViewContents is saved as an archetype in the space view's entity hierarchy.
        // This means, that the above already copied the space view contents!
        let contents = SpaceViewContents::new(
            new_id,
            self.class_identifier,
            self.contents.entity_path_filter.clone(),
        );

        Self {
            id: new_id,
            display_name: self.display_name.clone(),
            class_identifier: self.class_identifier,
            space_origin: self.space_origin.clone(),
            contents,
            visible: self.visible,
            pending_writes,
        }
    }

    pub fn clear(&self, ctx: &ViewerContext<'_>) {
        let clear = Clear::recursive();
        ctx.save_blueprint_component(&self.entity_path(), &clear.is_recursive);
    }

    #[inline]
    pub fn set_display_name(&self, ctx: &ViewerContext<'_>, name: Option<String>) {
        if name != self.display_name {
            match name {
                Some(name) => {
                    let component = Name(name.into());
                    ctx.save_blueprint_component(&self.entity_path(), &component);
                }
                None => {
                    ctx.save_empty_blueprint_component::<Name>(&self.entity_path());
                }
            }
        }
    }

    #[inline]
    pub fn set_origin(&self, ctx: &ViewerContext<'_>, origin: &EntityPath) {
        if origin != &self.space_origin {
            let component = SpaceViewOrigin(origin.into());
            ctx.save_blueprint_component(&self.entity_path(), &component);
        }
    }

    #[inline]
    pub fn set_visible(&self, ctx: &ViewerContext<'_>, visible: bool) {
        if visible != self.visible {
            let component = Visible(visible);
            ctx.save_blueprint_component(&self.entity_path(), &component);
        }
    }

    pub fn class_identifier(&self) -> &SpaceViewClassIdentifier {
        &self.class_identifier
    }

    pub fn class<'a>(
        &self,
        space_view_class_registry: &'a re_viewer_context::SpaceViewClassRegistry,
    ) -> &'a dyn DynSpaceViewClass {
        space_view_class_registry.get_class_or_log_error(&self.class_identifier)
    }

    pub fn on_frame_start(
        &self,
        ctx: &ViewerContext<'_>,
        view_state: &mut dyn SpaceViewState,
        view_props: &mut EntityPropertyMap,
    ) {
        let query_result = ctx.lookup_query_result(self.id).clone();

        let mut per_system_entities = PerSystemEntities::default();
        {
            re_tracing::profile_scope!("per_system_data_results");

            query_result.tree.visit(&mut |node| {
                for system in &node.data_result.visualizers {
                    per_system_entities
                        .entry(*system)
                        .or_default()
                        .insert(node.data_result.entity_path.clone());
                }
                true
            });
        }

        self.class(ctx.space_view_class_registry).on_frame_start(
            ctx,
            view_state,
            &per_system_entities,
            view_props,
        );
    }

    pub fn scene_ui(
        &self,
        view_state: &mut dyn SpaceViewState,
        ctx: &ViewerContext<'_>,
        ui: &mut egui::Ui,
        query: &ViewQuery<'_>,
        system_output: SystemExecutionOutput,
    ) {
        re_tracing::profile_function!();

        let class = self.class(ctx.space_view_class_registry);

        let root_data_result = self.root_data_result(ctx.store_context, ctx.blueprint_query);
        let props = root_data_result
            .individual_properties()
            .cloned()
            .unwrap_or_default();

        ui.scope(|ui| {
            class.ui(ctx, ui, view_state, &props, query, system_output);
        });
    }

    #[inline]
    pub fn entity_path(&self) -> EntityPath {
        self.id.as_entity_path()
    }

    pub fn root_data_result(&self, ctx: &StoreContext<'_>, query: &LatestAtQuery) -> DataResult {
        let entity_path = self.entity_path();

        let is_time_series = self.class_identifier == "Time Series";

        let mut individual_properties = ctx
            .blueprint
            .store()
            .query_latest_component_quiet::<EntityPropertiesComponent>(&self.entity_path(), query)
            .map(|result| result.value.0);

        // TODO(#4194): this should come from delegation to the space-view-class
        if individual_properties.is_none() && is_time_series {
            let mut time_series_defaults = EntityProperties::default();
            time_series_defaults.visible_history.nanos = VisibleHistory::ALL;
            time_series_defaults.visible_history.sequences = VisibleHistory::ALL;
            individual_properties = Some(time_series_defaults);
        }

        let mut accumulated_properties = individual_properties.clone().unwrap_or_default();

        if is_time_series {
            // TODO(#4194): enabled == false means use defaults
            if !accumulated_properties.visible_history.enabled {
                accumulated_properties.visible_history.enabled = true;
                accumulated_properties.visible_history.nanos = VisibleHistory::ALL;
                accumulated_properties.visible_history.sequences = VisibleHistory::ALL;
            }
        }

        DataResult {
            entity_path: entity_path.clone(),
            visualizers: Default::default(),
            tree_prefix_only: false,
            property_overrides: Some(PropertyOverrides {
                accumulated_properties,
                individual_properties,
                recursive_properties: Default::default(),
                component_overrides: Default::default(),
                recursive_override_path: entity_path.clone(),
                individual_override_path: entity_path,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::data_query::{DataQuery, PropertyResolver};
    use re_entity_db::EntityDb;
    use re_log_types::{DataCell, DataRow, EntityPathFilter, RowId, StoreId, TimePoint};
    use re_types::archetypes::Points3D;
    use re_viewer_context::{
        blueprint_timeline, IndicatedEntities, PerVisualizer, SpaceViewClassRegistry, StoreContext,
        VisualizableEntities,
    };

    use super::*;

    fn save_override(props: EntityProperties, path: &EntityPath, store: &mut EntityDb) {
        let component = EntityPropertiesComponent(props);
        let row = DataRow::from_cells1_sized(
            RowId::new(),
            path.clone(),
            TimePoint::timeless(),
            1,
            DataCell::from([component]),
        )
        .unwrap();

        store.add_data_row(row).unwrap();
    }

    #[test]
    fn test_overrides() {
        let space_view_class_registry = SpaceViewClassRegistry::default();
        let mut recording = EntityDb::new(StoreId::random(re_log_types::StoreKind::Recording));
        let mut blueprint = EntityDb::new(StoreId::random(re_log_types::StoreKind::Blueprint));

        let points = Points3D::new(vec![[1.0, 2.0, 3.0]]);

        for path in [
            "parent".into(),
            "parent/skip/child1".into(),
            "parent/skip/child2".into(),
        ] {
            let row = DataRow::from_archetype(RowId::new(), TimePoint::timeless(), path, &points)
                .unwrap();
            recording.add_data_row(row).ok();
        }

        let space_view = SpaceViewBlueprint::new(
            "3D".into(),
            &EntityPath::root(),
            EntityPathFilter::parse_forgiving(
                r"
                    + parent
                    + parent/skip/child1
                    + parent/skip/child2
                ",
            ),
        );

        let auto_properties = Default::default();

        let mut visualizable_entities = PerVisualizer::<VisualizableEntities>::default();
        visualizable_entities
            .0
            .entry("Points3D".into())
            .or_insert_with(|| {
                VisualizableEntities(
                    [
                        EntityPath::from("parent"),
                        EntityPath::from("parent/skipped/child1"),
                    ]
                    .into_iter()
                    .collect(),
                )
            });
        let indicated_entities_per_visualizer = PerVisualizer::<IndicatedEntities>(
            visualizable_entities
                .0
                .iter()
                .map(|(id, entities)| (*id, IndicatedEntities(entities.iter().cloned().collect())))
                .collect(),
        );

        let blueprint_query = LatestAtQuery::latest(blueprint_timeline());
        let contents = &space_view.contents;

        let resolver = contents.build_resolver(
            &space_view_class_registry,
            &space_view,
            &auto_properties,
            &visualizable_entities,
            &indicated_entities_per_visualizer,
        );

        // No overrides set. Everybody has default values.
        {
            let ctx = StoreContext {
                blueprint: &blueprint,
                recording: Some(&recording),
                all_recordings: vec![],
            };

            let mut query_result = contents.execute_query(&ctx, &visualizable_entities);
            resolver.update_overrides(&ctx, &blueprint_query, &mut query_result);

            let parent = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent"))
                .unwrap();
            let child1 = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent/skip/child1"))
                .unwrap();
            let child2 = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent/skip/child2"))
                .unwrap();

            for result in [parent, child1, child2] {
                assert_eq!(
                    result.accumulated_properties(),
                    &EntityProperties::default(),
                );
            }

            // Now, override visibility on parent individually.
            let mut overrides = parent.individual_properties().cloned().unwrap_or_default();
            overrides.visible = false;

            save_override(
                overrides,
                parent.individual_override_path().unwrap(),
                &mut blueprint,
            );
        }

        // Parent is not visible, but children are
        {
            let ctx = StoreContext {
                blueprint: &blueprint,
                recording: Some(&recording),
                all_recordings: vec![],
            };

            let mut query_result = contents.execute_query(&ctx, &visualizable_entities);
            resolver.update_overrides(&ctx, &blueprint_query, &mut query_result);

            let parent_group = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent"))
                .unwrap();
            let parent = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent"))
                .unwrap();
            let child1 = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent/skip/child1"))
                .unwrap();
            let child2 = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent/skip/child2"))
                .unwrap();

            assert!(!parent.accumulated_properties().visible);

            for result in [child1, child2] {
                assert!(result.accumulated_properties().visible);
            }

            // Override visibility on parent recursively.
            let mut overrides = parent_group
                .individual_properties()
                .cloned()
                .unwrap_or_default();
            overrides.visible = false;

            save_override(
                overrides,
                parent_group.recursive_override_path().unwrap(),
                &mut blueprint,
            );
        }

        // Nobody is visible
        {
            let ctx = StoreContext {
                blueprint: &blueprint,
                recording: Some(&recording),
                all_recordings: vec![],
            };

            let mut query_result = contents.execute_query(&ctx, &visualizable_entities);
            resolver.update_overrides(&ctx, &blueprint_query, &mut query_result);

            let parent = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent"))
                .unwrap();
            let child1 = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent/skip/child1"))
                .unwrap();
            let child2 = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent/skip/child2"))
                .unwrap();

            for result in [parent, child1, child2] {
                assert!(!result.accumulated_properties().visible);
            }
        }

        // Override visible range on root
        {
            let root = space_view.root_data_result(
                &StoreContext {
                    blueprint: &blueprint,
                    recording: Some(&recording),
                    all_recordings: vec![],
                },
                &blueprint_query,
            );
            let mut overrides = root.individual_properties().cloned().unwrap_or_default();
            overrides.visible_history.enabled = true;
            overrides.visible_history.nanos = VisibleHistory::ALL;

            save_override(
                overrides,
                root.recursive_override_path().unwrap(),
                &mut blueprint,
            );
        }

        // Everyone has visible history
        {
            let ctx = StoreContext {
                blueprint: &blueprint,
                recording: Some(&recording),
                all_recordings: vec![],
            };
            let mut query_result = contents.execute_query(&ctx, &visualizable_entities);
            resolver.update_overrides(&ctx, &blueprint_query, &mut query_result);

            let parent = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent"))
                .unwrap();
            let child1 = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent/skip/child1"))
                .unwrap();
            let child2 = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent/skip/child2"))
                .unwrap();

            for result in [parent, child1, child2] {
                assert!(result.accumulated_properties().visible_history.enabled);
                assert_eq!(
                    result.accumulated_properties().visible_history.nanos,
                    VisibleHistory::ALL
                );
            }

            let mut overrides = child2.individual_properties().cloned().unwrap_or_default();
            overrides.visible_history.enabled = true;

            save_override(
                overrides,
                child2.individual_override_path().unwrap(),
                &mut blueprint,
            );
        }

        // Child2 has its own visible history
        {
            let ctx = StoreContext {
                blueprint: &blueprint,
                recording: Some(&recording),
                all_recordings: vec![],
            };

            let mut query_result = contents.execute_query(&ctx, &visualizable_entities);
            resolver.update_overrides(&ctx, &blueprint_query, &mut query_result);

            let parent = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent"))
                .unwrap();
            let child1 = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent/skip/child1"))
                .unwrap();
            let child2 = query_result
                .tree
                .lookup_result_by_path(&EntityPath::from("parent/skip/child2"))
                .unwrap();

            for result in [parent, child1] {
                assert!(result.accumulated_properties().visible_history.enabled);
                assert_eq!(
                    result.accumulated_properties().visible_history.nanos,
                    VisibleHistory::ALL
                );
            }

            assert!(child2.accumulated_properties().visible_history.enabled);
            assert_eq!(
                child2.accumulated_properties().visible_history.nanos,
                VisibleHistory::OFF
            );
        }
    }
}
