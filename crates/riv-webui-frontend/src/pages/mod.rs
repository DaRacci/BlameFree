// pub mod adhoc_review;
// pub mod adhoc_runs;
pub mod admin;
pub mod four_zero_four;
pub mod home;
// pub mod live;
// pub mod new_benchmark;
// pub mod new_run;
// pub mod pr_detail;
// pub mod run_detail;

/// Declare a signal struct with auto-generated `ReadSignal<T>` / `WriteSignal<T>` pairs.
///
/// Each field becomes two struct fields: `field: ReadSignal<T>` and `set_field: WriteSignal<T>`.
/// The generated `new()` constructor calls `signal(default)` for every field.
///
/// Fields listed after `write_only { ... }` emit only `WriteSignal<T>` (no `ReadSignal<T>`).
///
/// # Example
///
/// ```ignore
/// signal_struct! {
///     struct MySignals {
///         name: String = String::new(),
///         count: u32 = 0,
///     }
///     write_only {
///         set_result: Option<String> = None,
///     }
/// }
/// ```
#[macro_export]
macro_rules! signal_struct {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $field:ident : $ty:ty = $default:expr
            ),* $(,)?
        }
        $(write_only {
            $(
                $wo_setter:ident : $wo_ty:ty = $wo_default:expr
            ),* $(,)?
        })?
    ) => {
        ::paste::paste! {
            $(#[$meta])*
            #[derive(Clone, Copy)]
            $vis struct $name {
                $(
                    $field: ::leptos::prelude::ReadSignal<$ty>,
                    [<set_ $field>]: ::leptos::prelude::WriteSignal<$ty>,
                )*
                $($(
                    $wo_setter: ::leptos::prelude::WriteSignal<$wo_ty>,
                )*)?
            }

            impl $name {
                #[allow(unused_mut)]
                fn new() -> Self {
                    $(
                        let ($field, [<set_ $field>]) = ::leptos::prelude::signal($default);
                    )*
                    $($(
                        let (_, $wo_setter) = ::leptos::prelude::signal($wo_default);
                    )*)?
                    Self {
                        $($field, [<set_ $field>],)*
                        $($($wo_setter,)*)?
                    }
                }
            }
        }
    };
}
