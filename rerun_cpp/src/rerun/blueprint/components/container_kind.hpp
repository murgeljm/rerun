// DO NOT EDIT! This file was auto-generated by crates/re_types_builder/src/codegen/cpp/mod.rs
// Based on "crates/re_types/definitions/rerun/blueprint/components/container_kind.fbs".

#pragma once

#include "../../result.hpp"

#include <cstdint>
#include <memory>

namespace arrow {
    class Array;
    class DataType;
    class SparseUnionBuilder;
} // namespace arrow

namespace rerun::blueprint::components {
    /// **Component**: The kind of a blueprint container (tabs, grid, …).
    enum class ContainerKind : uint8_t {

        Tabs = 1,

        Horizontal = 2,

        Vertical = 3,

        Grid = 4,
    };
} // namespace rerun::blueprint::components

namespace rerun {
    template <typename T>
    struct Loggable;

    /// \private
    template <>
    struct Loggable<blueprint::components::ContainerKind> {
        static constexpr const char Name[] = "rerun.blueprint.components.ContainerKind";

        /// Returns the arrow data type this type corresponds to.
        static const std::shared_ptr<arrow::DataType>& arrow_datatype();

        /// Fills an arrow array builder with an array of this type.
        static rerun::Error fill_arrow_array_builder(
            arrow::SparseUnionBuilder* builder,
            const blueprint::components::ContainerKind* elements, size_t num_elements
        );

        /// Serializes an array of `rerun::blueprint:: components::ContainerKind` into an arrow array.
        static Result<std::shared_ptr<arrow::Array>> to_arrow(
            const blueprint::components::ContainerKind* instances, size_t num_instances
        );
    };
} // namespace rerun
