#[macro_export]
macro_rules! all_arg_constructor {
    (struct $name:ident { $( $field:ident : $ty:ty ),* $(,)? }) => {
        struct $name {
            $(
            $field: $ty,
            )*
        }
        impl $name {
            pub fn new($( $field: $ty ),*) -> Self {
                Self { $( $field ),* }
            }
        }
    };
    (pub struct $name:ident { $( $field:ident : $ty:ty ),* $(,)? }) => {
        pub struct $name {
            $(
            $field: $ty,
            )*
        }
        impl $name {
            pub fn new($( $field: $ty ),*) -> Self {
                Self { $( $field ),* }
            }
        }
    };
}

#[macro_export]
macro_rules! arc_mutex {
    ($elem:expr) => {
        ::std::sync::Arc::new(::std::sync::Mutex::new($elem))
    };
}
