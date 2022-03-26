use labeled::dclabel::DCLabel;

use crate::labeled_fs;

const ROOT: &str = "/";

/// Utility function to create function directory under the root directory
pub fn create_root_function_dir(name: &str) {
    let mut cur_label = DCLabel::bottom();
    labeled_fs::create_dir(ROOT, name, DCLabel::new(true, [[name]]), &mut cur_label, DCLabel::top()).unwrap();
}

/// Utility function to create user directory under the root directory
pub fn create_root_user_dir(user: &str) {
    let mut cur_label = DCLabel::bottom();
    labeled_fs::create_dir(ROOT, user, DCLabel::new([[user]], [[user]]), &mut cur_label, DCLabel::top()).unwrap();
}
