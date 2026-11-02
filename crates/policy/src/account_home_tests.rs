use super::account_home;

/// The account record names a real directory the account owns, whatever
/// the test process's `HOME` or `USERPROFILE` says.
#[test]
fn the_account_home_is_a_directory_the_account_owns() {
    let home = account_home().expect("the account has a home directory");
    assert!(home.is_absolute(), "{home:?}");
    assert!(home.is_dir(), "{home:?}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let owner = std::fs::metadata(&home).expect("home metadata").uid();
        assert_eq!(owner, unsafe { libc::geteuid() }, "{home:?}");
    }
}
