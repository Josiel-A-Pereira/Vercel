use clap::Args;

#[derive(Args, Debug, Default)]
pub(crate) struct InteractionArgs {
    #[arg(
        long,
        global = true,
        help = "Prefer physical delivery for natural input commands and permit focus/cursor side effects. Default forbids focus/cursor side effects; targeted keys follow platform rules."
    )]
    pub headed: bool,
}
