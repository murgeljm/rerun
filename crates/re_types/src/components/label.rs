// NOTE: This file was autogenerated by re_types_builder; DO NOT EDIT.

#[doc = "A String label component."]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Label(pub String);

impl crate::Component for Label {
    fn name() -> crate::ComponentName {
        crate::ComponentName::Borrowed("rerun.components.Label")
    }

    #[allow(clippy::wildcard_imports)]
    fn to_arrow_datatype() -> arrow2::datatypes::DataType {
        use ::arrow2::datatypes::*;
        DataType::Utf8
    }
}
