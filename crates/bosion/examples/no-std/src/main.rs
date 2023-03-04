#![cfg_attr(not(test), no_main)]
#![cfg_attr(not(test), no_std)]

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
    loop {}
}

include!(env!("BOSION_PATH"));

#[cfg(test)]
#[path = "../../default/src/common.rs"]
mod common;

#[cfg(test)]
mod test {
	use super::*;

	test_snapshot!(crate_version, Bosion::CRATE_VERSION);

	test_snapshot!(crate_features, format!("{:#?}", Bosion::CRATE_FEATURES));

	test_snapshot!(build_date, Bosion::BUILD_DATE);

	test_snapshot!(build_datetime, Bosion::BUILD_DATETIME);

	test_snapshot!(no_git_long_version, Bosion::LONG_VERSION);
}
