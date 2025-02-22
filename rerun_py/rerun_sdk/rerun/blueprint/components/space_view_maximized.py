# DO NOT EDIT! This file was auto-generated by crates/re_types_builder/src/codegen/python/mod.rs
# Based on "crates/re_types/definitions/rerun/blueprint/components/space_view_maximized.fbs".

# You can extend this class by creating a "SpaceViewMaximizedExt" class in "space_view_maximized_ext.py".

from __future__ import annotations

from ... import datatypes
from ..._baseclasses import ComponentBatchMixin

__all__ = ["SpaceViewMaximized", "SpaceViewMaximizedBatch", "SpaceViewMaximizedType"]


class SpaceViewMaximized(datatypes.Uuid):
    """
    **Component**: Whether a space view is maximized.

    Unstable. Used for the ongoing blueprint experimentations.
    """

    # You can define your own __init__ function as a member of SpaceViewMaximizedExt in space_view_maximized_ext.py

    # Note: there are no fields here because SpaceViewMaximized delegates to datatypes.Uuid
    pass


class SpaceViewMaximizedType(datatypes.UuidType):
    _TYPE_NAME: str = "rerun.blueprint.components.SpaceViewMaximized"


class SpaceViewMaximizedBatch(datatypes.UuidBatch, ComponentBatchMixin):
    _ARROW_TYPE = SpaceViewMaximizedType()
