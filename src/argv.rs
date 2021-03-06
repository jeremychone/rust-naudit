use clap::{crate_version, App, Arg};

pub fn cmd_app() -> App<'static> {
	let app = App::new("naudit")
		.version(&crate_version!()[..])
		.about("npm multi package audit")
		.arg(
			Arg::new("clean")
				.short('c')
				.long("clean")
				.help("clean the node_modules and package-lock.json")
				.takes_value(false),
		)
		.arg(
			Arg::new("no_install")
				.long("no-install")
				.help("Do not do a npm install")
				.takes_value(false),
		)
		.arg(
			Arg::new("no_audit")
				.long("no-audit")
				.help("Do not do a npm audit")
				.takes_value(false),
		)
		.arg(Arg::new("PATH").help("Path to root"));

	app
}
