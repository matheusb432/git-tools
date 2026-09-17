use std::ffi::OsStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowLaunch {
    Application,
    DiffForeground,
    DiffBackground,
}

impl WindowLaunch {
    pub(super) fn from_arguments(arguments: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        let mut launch = Self::Application;
        for argument in arguments {
            if argument.as_ref() == "--background-diff" {
                return Self::DiffBackground;
            }
            if argument.as_ref() == "--focus-diff" {
                launch = Self::DiffForeground;
            }
        }
        launch
    }

    pub(super) const fn focuses_window(self) -> bool {
        !matches!(self, Self::DiffBackground)
    }

    pub(super) const fn opens_diff(self) -> bool {
        !matches!(self, Self::Application)
    }
}

#[cfg(test)]
mod tests {
    use super::WindowLaunch;

    #[test]
    fn launch_arguments_keep_navigation_separate_from_window_activation() {
        for (arguments, opens_diff, focuses_window) in [
            (vec!["gtl-viewer"], false, true),
            (vec!["gtl-viewer", "--focus-diff"], true, true),
            (vec!["gtl-viewer", "--background-diff"], true, false),
            (
                vec!["gtl-viewer", "--focus-diff", "--background-diff"],
                true,
                false,
            ),
        ] {
            let launch = WindowLaunch::from_arguments(arguments);
            assert_eq!(launch.opens_diff(), opens_diff);
            assert_eq!(launch.focuses_window(), focuses_window);
        }
    }
}
