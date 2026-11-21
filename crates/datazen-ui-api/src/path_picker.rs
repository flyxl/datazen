#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathPickerFileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PickPathInput {
    pub directory: bool,
    pub multiple: bool,
    pub title: Option<String>,
    pub default_path: Option<String>,
    pub filters: Vec<PathPickerFileFilter>,
}

pub struct PathPicker {
    pick: Box<dyn Fn(Option<&PickPathInput>) -> Result<Option<String>, String>>,
}

impl PathPicker {
    pub fn new(
        pick: impl Fn(Option<&PickPathInput>) -> Result<Option<String>, String> + 'static,
    ) -> Self {
        Self { pick: Box::new(pick) }
    }

    pub fn pick_path(&self, options: Option<&PickPathInput>) -> Result<Option<String>, String> {
        (self.pick)(options)
    }
}

pub fn pick_path(
    picker: &PathPicker,
    options: Option<&PickPathInput>,
) -> Result<Option<String>, String> {
    picker.pick_path(options)
}

pub fn web_path_picker() -> PathPicker {
    PathPicker::new(|_| Ok(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_are_all_absent_or_false() {
        let options = PickPathInput::default();
        assert!(!options.directory);
        assert!(!options.multiple);
        assert_eq!(options.title, None);
        assert_eq!(options.default_path, None);
        assert!(options.filters.is_empty());
    }

    #[test]
    fn the_accessor_forwards_options_to_the_host_capability() {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(None));
        let recorder = seen.clone();
        let picker = PathPicker::new(move |options| {
            recorder.replace(options.cloned());
            Ok(Some("/tmp/picked.txt".to_owned()))
        });
        let options = PickPathInput {
            directory: true,
            filters: vec![PathPickerFileFilter {
                name: "SQL".to_owned(),
                extensions: vec!["sql".to_owned()],
            }],
            ..Default::default()
        };
        assert_eq!(pick_path(&picker, Some(&options)), Ok(Some("/tmp/picked.txt".to_owned())));
        assert_eq!(seen.borrow().as_ref(), Some(&options));
    }

    #[test]
    fn a_missing_options_argument_is_forwarded_as_none() {
        let picker = PathPicker::new(|options| match options {
            None => Ok(Some("/tmp/default".to_owned())),
            Some(_) => Ok(None),
        });
        assert_eq!(pick_path(&picker, None), Ok(Some("/tmp/default".to_owned())));
    }

    #[test]
    fn cancellation_and_host_errors_are_distinct() {
        let cancelled = PathPicker::new(|_| Ok(None));
        assert_eq!(pick_path(&cancelled, None), Ok(None));
        let failed = PathPicker::new(|_| Err("dialog plugin unavailable".to_owned()));
        assert_eq!(pick_path(&failed, None), Err("dialog plugin unavailable".to_owned()));
    }

    #[test]
    fn the_web_form_has_no_native_picker_and_reports_cancel() {
        let picker = web_path_picker();
        assert_eq!(pick_path(&picker, None), Ok(None));
        assert_eq!(
            pick_path(&picker, Some(&PickPathInput { directory: true, ..Default::default() })),
            Ok(None)
        );
    }
}
