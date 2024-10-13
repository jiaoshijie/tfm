use simplelog::LevelFilter;
use std::sync::RwLock;

use crate::dir::SortType;

pub const LOG_LEVEL: LevelFilter = LevelFilter::Warn;
pub const LOG_FILE_PATH: &str = "~/.cache/rust-tfm/log"; // CAN'T use env variables in this path

// The `RUST_TFM` can be used for detecting whether you are in a tfm->bash(mean you are using tfm to open a interactive bash to work) or only a bash
// You can set bash PS1 to contain this `RUST_TFM`, so that you can always detect whether you have a blocked tfm process.
pub const RUST_TFM: &str = "(rust-tfm)";

pub const SCROLL_OFF: u16 = 6;
pub const CASE_INSENSITIVE: bool = true;
pub const WORD_SEPS: &[char] = &['/', '.']; // And all non-printed characters

// default
pub static HIDDEN: RwLock<bool> = RwLock::new(true); // true: don't show hidden file by default
pub static SORT_TYPE: RwLock<SortType> = RwLock::new(SortType::Natural); // using Natural order by default

// NOTE: You need to change these pathes to your own pathes.
pub const PREVIEWER: &str = "~/code/tfm/script/preview";
pub const OPENER: &str = "~/code/tfm/script/open";

#[rustfmt::skip]
pub mod actions {
    // NOTE: (&str, &str, &str): (`c` for CallAction and `s` for SetAction and `m` for CmdAction, keymap, operation)
    pub const KEYS: &[(&str, &str, &str)] = &[
        ("c", "j", "down"),
        ("c", "k", "up"),
        ("c", "gg", "top"),
        ("c", "G", "bottom"),
        ("c", "h", "updir"),
        ("c", "l", "open"),   // Enter directory or open a file using `OPENER` defined above.
        ("c", "q", "quit"),   // quit tfm
        ("c", "s", "shell"),  // enable tfm command mode to run a simple shell command.
        ("c", "S", "Shell"),  // block tfm and open a interactive shell using `$SHELL` or `bash`
        ("c", "<C-l>", "redraw"),  // redraw tfm ui. Because some operations maybe not update in time or file or directory changes outside, you can use `redraw` to manually update tfm.
        // run tfm command(cd, set, rename, q, quit, quit!)
        ("c", ":", "command_mode"),
        // run bash command
        ("c", "$", "command_mode"),  // block tfm but when command finished, immediately resuming tfm. Usage: `mkdir test`
        ("c", "!", "command_mode"),  // block tfm when command finished, waiting user to input a key then resuming tfm. Usage: `echo $PWD`
        ("c", "&", "command_mode"),  // not block tfm. Usage: `echo 'something' | xsel -ib`
        // search file
        ("c", "/", "command_mode"),
        ("c", "?", "command_mode"),
        ("c", "n", "search_next"),
        ("c", "N", "search_prev"),

        ("c", " ", "toggle"),  // selection
        ("c", "v", "toggle_all"),
        ("c", "u", "unselect"),

        ("c", "dd", "cut"),  // Only selection(mark as cut), don't do anything
        ("c", "yy", "copy"),  // Only selection(mark as copy), don't do anything
        ("c", "c", "clear"),  // Clear `cut` or `copy` selection

        ("c", "pp", "paste"),   // built-in paste function(move or copy depending on `dd` or `yy`).
        ("c", "DD", "remove"),  // NOTE: this is the built-in rm function. It's recommended using a `trash` instead of this function, like `dD` below does.

        ("c", "a", "rename"),  // rename file under the cursor
        // --------------------------
        ("s", "zh", "hidden!"),  // toggle hidden files
        ("s", "zs", "sortby size"),  // sort by file size
        ("s", "zn", "sortby natural"),  // sort by natural comparison
        // --------------------------
        ("m", "gt", ":cd /tmp"),  // goto /tmp
        ("m", "gh", ":cd /home/jsj"),  // goto home directory, NOTE: you need change to your own dir.
        ("m", "gu", ":cd /usr"),
        ("m", "gc", ":cd /home/jsj/.config"),  // NOTE: you need change to your own dir.
        ("m", "gl", ":cd /home/jsj/.local"),   // NOTE: you need change to your own dir.
        ("m", "gr", ":cd /"),

        ("m", "bg", "$set-bg \"$rust_tfm_f\""),  // NOTE: this is a bash script to set backgrounp only for myself, you can just delete it.
        ("m", "<Enter>", "$$EDITOR \"$rust_tfm_f\""),  // NOTE: Open a file using text editor, and don't care what file type it is.
        ("m", "dD", "$IFS=$'\n';trashy put $rust_tfm_fx"),  // NOTE: using `trashy put` to remove files to Trash instead directly remove it.
        ("m", "A", CMD_CHANGE_FILES_NAME), // change files name using text editor, NOTE: when changing finished you need manually unselect previously selected files.

        ("m", "yp", "&echo -n \"$rust_tfm_f\" | xsel -ib"),  // copy file path under the cursor to system clipboard. NOTE: you need to change `xsel` to your clipboard manager(X11: xclip or xsel, Wayland: wl-clipboard)
        ("m", "yn", "&echo -n \"$rust_tfm_f\" | awk -F '/' '{printf $NF}' | xsel -ib"),  // copy file name to system clipboard
        ("m", "y.", "&echo -n \"$rust_tfm_f\" | awk -F '/' '{printf $NF}' | sed \"s/\\.[^\\.]*$//g\" | xsel -ib"),  // copy file name(without extension) to system clipboard

        ("m", "pf", "!echo \"$rust_tfm_f\""),
    ];

    const CMD_CHANGE_FILES_NAME: &str = r#"${
        tfm_tmp_file=$(mktemp /tmp/tfm_vim_change_files_name.XXXXXXXXXX)
        rm -rf $tfm_tmp_file
        $EDITOR $tfm_tmp_file -c 'execute ".!printenv rust_tfm_fx"'
        [ -f "$tfm_tmp_file" ] || exit

        tfm_file_number=$(wc -l < "$tfm_tmp_file")
        tfm_dst_file_con=($(cat $tfm_tmp_file))
        tfm_src_file_con=($rust_tfm_fx)

        if [ "${#tfm_src_file_con[@]}" -eq "$tfm_file_number" ]; then
        for ((i = 0; i < $tfm_file_number; i++)); do
            src=${tfm_src_file_con[$i]}
            dst=${tfm_dst_file_con[$i]}
            [ "$src" == "$dst" ] || [ -e "$dst" ] || mv -n -- "$src" "$dst"
        done
        fi
        rm -rf $tfm_tmp_file
    }"#;
}

#[rustfmt::skip]
pub mod theme {
    use crate::buffer::{Attr, Attrs, Style};
    use crossterm::style::Color;
    macro_rules! set_style {
        ($name:ident, $fg:expr, $bg:expr, $attrs:expr$(,)?) => {
            pub const $name: Style = Style {
                fg: $fg,
                bg: $bg,
                attrs: $attrs,
            };
        };
    }

    set_style!(USER_STYLE, Color::Yellow, Color::Reset, Attrs(Attr::Bold as u8));

    set_style!(EXECUTABLE_FILE_STYLE, Color::Green                  , Color::Reset, Attrs(Attr::Bold as u8 | Attr::Italic as u8));
    set_style!(REG_FILE_STYLE       , Color::White                  , Color::Reset, Attrs(0u8));
    set_style!(DIR_STYLE            , Color::Blue                   , Color::Reset, Attrs(Attr::Bold as u8));
    set_style!(LINK_WORKING_STYLE   , Color::Cyan                   , Color::Reset, Attrs(Attr::Bold as u8));
    set_style!(LINK_BROKEN_STYLE    , Color::Rgb{r: 255, g: 0, b: 0}, Color::Reset, Attrs(0u8));
    set_style!(PIPE_FILE_STYLE      , Color::DarkYellow             , Color::Reset, Attrs(0u8));
    set_style!(SOCKET_FILE_STYLE    , Color::Magenta                , Color::Reset, Attrs(Attr::Bold as u8));
    set_style!(CHAR_FILE_STYLE      , Color::Yellow                 , Color::Reset, Attrs(Attr::Bold as u8));
    set_style!(BLOCK_FILE_STYLE     , Color::Yellow                 , Color::Reset, Attrs(Attr::Bold as u8));

    set_style!(UI_BORDER_STYLE, Color::Grey, Color::Reset, Attrs(0u8));

    set_style!(WARN_MSG_STYLE, Color::AnsiValue(232), Color::White, Attrs(0u8));
    set_style!(ERROR_MSG_STYLE, Color::Rgb{r: 0, g: 0, b: 0}, Color::Rgb{r: 255, g: 0, b: 0}, Attrs(Attr::Bold as u8 | Attr::Underline as u8));

    set_style!(FILE_INFO_STYLE, Color::White, Color::Reset, Attrs(0u8));
    set_style!(PROPORTION_STYLE, Color::White, Color::Reset, Attrs(0u8));
    set_style!(PROGRESS_STYLE, Color::Black, Color::Cyan, Attrs(0u8));

    set_style!(SELECTION_STYLE, Color::Black, Color::Magenta, Attrs(0u8));
    set_style!(CUT_STYLE, Color::Black, Color::Red, Attrs(0u8));
    set_style!(COPY_STYLE, Color::Black, Color::Yellow, Attrs(0u8));
}
