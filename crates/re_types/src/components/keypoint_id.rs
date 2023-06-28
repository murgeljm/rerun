// NOTE: This file was autogenerated by re_types_builder; DO NOT EDIT.

#[doc = "A 16-bit ID representing a type of semantic keypoint within a class."]
#[doc = ""]
#[doc = "`KeypointId`s are only meaningful within the context of a `crate::components::ClassDescription`."]
#[doc = ""]
#[doc = "Used to look up an `crate::components::AnnotationInfo` for a Keypoint within the `crate::components::AnnotationContext`."]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeypointId(pub u16);

impl crate::Component for KeypointId {
    fn name() -> crate::ComponentName {
        crate::ComponentName::Borrowed("rerun.components.KeypointId")
    }

    #[allow(clippy::wildcard_imports)]
    fn to_arrow_datatype() -> arrow2::datatypes::DataType {
        use ::arrow2::datatypes::*;
        DataType::UInt16
    }
}
