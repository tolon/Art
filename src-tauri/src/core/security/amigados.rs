//! What may never appear in a string ART interpolates into an AmigaDOS
//! command line.
//!
//! ART generates AmigaDOS scripts in more than one place now — the WHDLoad
//! boot directory behind "one click starts the game"
//! ([`crate::core::launch::whdload_boot`]) and the work volume that runs a
//! package's own installer ([`crate::core::amigainstall::workvol`]). A script
//! is a command interpreter's input, so both are the same hazard, and a guard
//! that exists twice is a guard that will diverge. It lives here, in the
//! module whose charter is hostile *input*, so there is one answer to "what is
//! refused" rather than one per generator.
//!
//! The refusal is the one established at ART-118 and its sources are kept with
//! it below.

use crate::core::error::{CoreError, CoreResult};

/// Refuse a value that would change what a generated AmigaShell command line
/// *does*, rather than merely what it names.
///
/// Every value a generator interpolates can carry an attack — a program name,
/// an argument, and a volume label alike — so all of them come through here.
/// The set refused:
///
/// - a control character (`\n` or `\r` starts a new script line — a name
///   ending `...slave\nDelete DH0:#?` adds a command of its own);
/// - `"` (opens a quoted string, changing where the current one ends);
/// - `*` — AmigaDOS's escape character, which cancels the special meaning of
///   whatever follows it;
/// - `;` — separates a command from a comment on an AmigaShell line, and so
///   ends the command that was being built;
/// - `>` and `<` — redirect a command's output or input. `>` is how
///   `Turrican.slave >DH1:C/something` turns `WHDLoad`'s own command line
///   into a redirection that overwrites an arbitrary file on the game
///   volume, which is mounted **writable on purpose** so WHDLoad can keep
///   save games there. `>>` (append) is already refused because it contains
///   `>`. The same reasoning binds harder for an install run, whose whole
///   purpose is a writable system volume;
/// - `` ` `` and `$` — refused on a confirmed mechanism, not suspicion. The
///   AmigaOS Manual's "AmigaDOS Using Scripts" chapter (wiki.amigaos.net)
///   states that `$` "introduces an environment variable (which also works
///   outside of a script)", and that "back apostrophes are used to execute
///   commands from within a string" — "if a string containing a command
///   enclosed in back apostrophe is printed, the enclosed command is
///   executed." Both are real Shell features, at the same command-line
///   level `;`/`>`/`<` act at, so either is as dangerous as those three.
///   One reservation the source does not settle: it describes backtick
///   substitution happening *within a string*, while these values are
///   interpolated **unquoted** into the generated line, so whether an
///   unquoted occurrence is substituted the same way is not established by
///   that chapter. The refusal is correct either way — neither a WHDLoad
///   slave name nor an installer's path needs either character — so this
///   refuses rather than resolve that open question first.
///
/// Considered and not added: AmigaDOS's pattern-matching wildcards (`#?`,
/// `%`, `(a|b)`) are interpreted per-command by whichever program chooses to
/// treat its own argument as a pattern — unlike a Unix shell, AmigaDOS does
/// not expand them while parsing the command line, so they cannot change
/// *which* command runs or *where its output goes*, only how one already-
/// chosen command might later read its own argument. That reasoning does
/// not extend to backtick and `$`, which is exactly why those two are
/// refused above instead of joining this list.
///
/// This refuses; it never sanitises. A generator that silently rewrites what
/// it was told to run is not one to trust with a startup-sequence.
pub fn refuse_shell_metacharacters(label: &str, value: &str) -> CoreResult<()> {
    if value
        .chars()
        .any(|c| c.is_control() || matches!(c, '"' | '*' | ';' | '>' | '<' | '`' | '$'))
    {
        return Err(CoreError::InvalidInput(format!(
            "'{value}' is not a valid {label}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plain case: an ordinary name is not refused, so the tests below
    /// are refusing something rather than refusing everything.
    #[test]
    fn an_ordinary_value_passes() {
        refuse_shell_metacharacters("test value", "Turrican.slave").unwrap();
        refuse_shell_metacharacters("test value", "PKG:C/Updater").unwrap();
        refuse_shell_metacharacters("test value", "Workbench3.9").unwrap();
    }

    /// One assertion per refused character, so a character dropped from the
    /// set fails one named test rather than none.
    #[test]
    fn every_character_in_the_set_is_refused() {
        for hostile in [
            "name\nDelete DH0:#?",
            "name\rFormat",
            "name\"quoted",
            "name*escaped",
            "name;second",
            "name >DH1:overwritten",
            "name <DH1:secret",
            "name `Format DH0:`",
            "name $evil",
        ] {
            assert!(
                refuse_shell_metacharacters("test value", hostile).is_err(),
                "{hostile} should be refused"
            );
        }
    }

    /// AmigaDOS wildcards are not refused, and the doc comment says why —
    /// pinning it here so removing that reasoning breaks a test.
    #[test]
    fn amigados_wildcards_are_not_refused() {
        refuse_shell_metacharacters("test value", "Update#?").unwrap();
    }
}
