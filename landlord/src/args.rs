pub enum ExecutionMode {
    Class { class: String, args: Vec<String> },
    Exit { code: i32 },
    Help { code: i32 },
    JarFile { file: String, args: Vec<String> },
}

pub struct JavaArgs {
    pub cp: Vec<String>,
    pub errors: Vec<String>,
    pub mode: ExecutionMode,
    pub props: Vec<(String, String)>,
    pub socket: String,
    pub version: bool,
}

pub fn parse_java_args(args: &Vec<String>) -> JavaArgs {
    // We want to aim to be a drop-in replacement for java, so we have to roll our own arg parser
    // because DocOpt/Clap/et al don't have the required features to match the rather strange java
    // arguments.

    let noop_flags = vec![
        "-server".to_string(),
        "-d64".to_string(),
        "-d32".to_string()
    ];

    let mut jargs = JavaArgs {
        cp: vec![".".to_string()],
        errors: vec![],
        mode: ExecutionMode::Help { code: 1 },
        props: vec![],
        socket: "/var/run/landlord/landlordd.sock".to_string(),
        version: false,
    };

    let mut args_left = args.clone();

    while !args_left.is_empty() {
        match args_left.remove(0) {
            ref entry if !entry.starts_with("-") => {
                let mut args = vec![];

                while !args_left.is_empty() {
                    args.push(args_left.remove(0));
                }

                jargs.mode = ExecutionMode::Class { class: entry.to_string(), args: args };
            },

            ref flag if flag == "-jar" => {
                if !args_left.is_empty() {
                    let mut args = vec![];
                    let file = args_left.remove(0);

                    while !args_left.is_empty() {
                        args.push(args_left.remove(0));
                    }

                    jargs.mode = ExecutionMode::JarFile { file, args };
                } else {
                    jargs.errors.push(format!("{} requires jar file specification", flag))
                }
            },

            ref flag if flag == "-?" || flag == "-help" => {
                jargs.mode = ExecutionMode::Help { code: 0 };
            },

            ref flag if flag == "-version" => {
                jargs.version = true;
                jargs.mode = ExecutionMode::Exit { code: 0 };
            },

            ref flag if flag == "-showversion" => {
                jargs.version = true;
            },

            ref flag if flag == "-cp" || flag == "-classpath" => {
                if !args_left.is_empty() {
                    jargs.cp = args_left.remove(0).split(":").map(|s| s.to_string()).collect();
                } else {
                    jargs.errors.push(format!("{} requires class path specification", flag))
                }
            },

            ref flag if flag == "-socket" => {
                if !args_left.is_empty() {
                    jargs.socket = args_left.remove(0);
                } else {
                    jargs.errors.push(format!("{} requires socket specification", flag))
                }
            },

            ref flag if flag.starts_with("-D") => {
                if let Some(s) = flag.get(2..) {
                    let parts: Vec<&str> = s.splitn(2, "=").collect();

                    if parts.len() == 2 {
                        jargs.props.push((parts[0].to_string(), parts[1].to_string()));
                    }
                }
            },

            ref flag if noop_flags.contains(flag) => {},

            flag => jargs.errors.push(format!("Unrecognized option: {}", flag))
        };
    }

    jargs
}

pub fn validate_java_args(args: &JavaArgs) -> Vec<String> {
    let mut all_errors = args.errors.clone();

    let maybe_errors: Vec<Option<&str>> = vec![];

    all_errors.append(
        &mut maybe_errors
            .iter()
            .filter_map(|error| error.map(|message| message.to_string()))
            .collect());

    all_errors
}
