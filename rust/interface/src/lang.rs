//! Os textos das telas em português do Brasil e inglês (os dois únicos idiomas). O idioma vem do
//! `config.ini` (`[interface] idioma = pt-br | en`); pt-br é o padrão.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    PtBr,
    En,
}

impl Language {
    /// Lê o valor do `config.ini` (sem diferença de maiúsculas; o resto cai no padrão).
    pub fn from_config(v: &str) -> Self {
        let v = v.trim();
        if v.eq_ignore_ascii_case("en") || v.eq_ignore_ascii_case("en-us") {
            Language::En
        } else {
            Language::PtBr
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Language::PtBr => "pt-br",
            Language::En => "en",
        }
    }

    pub fn texts(self) -> &'static Texts {
        match self {
            Language::PtBr => &PT_BR,
            Language::En => &EN,
        }
    }
}

/// Todos os textos que aparecem na tela.
pub struct Texts {
    // nomes das telas (coluna lateral)
    pub systems: &'static str,
    pub games: &'static str,
    pub menu: &'static str,
    // tela inicial
    pub favorites: &'static str,
    pub recent: &'static str,
    pub settings: &'static str,
    pub coming_soon: &'static str,
    pub no_folder: &'static str,
    /// "1 jogo"
    pub game_one: &'static str,
    /// "N jogos" (vem depois do número)
    pub games_n: &'static str,
    // lista
    pub folder: &'static str,
    pub no_games: &'static str,
    pub copy_to: &'static str,
    // avisos
    pub mapper: &'static str,
    pub not_supported: &'static str,
    pub needs: &'static str,
    pub inside: &'static str,
    pub invalid: &'static str,
    // legenda (ação de cada botão)
    pub open: &'static str,
    pub enter: &'static str,
    pub back: &'static str,
    pub favorite: &'static str,
    /// "ajustes" na legenda (minúsculo, como as outras ações)
    pub settings_action: &'static str,
    pub letter: &'static str,
    pub ok: &'static str,
    pub back_to_game: &'static str,
    // menu do jogo
    pub resume: &'static str,
    pub reset: &'static str,
    pub save_state: &'static str,
    pub load_state: &'static str,
    pub shader: &'static str,
    pub quit_to_menu: &'static str,
    // ajustes
    pub colors: &'static str,
    pub background: &'static str,
    pub lang: &'static str,
    pub lang_name: &'static str,
    pub video_mode: &'static str,
    pub change: &'static str,
}

pub const PT_BR: Texts = Texts {
    systems: "Sistemas",
    games: "Jogos",
    menu: "Menu",
    favorites: "Favoritos",
    recent: "Recentes",
    settings: "Ajustes",
    coming_soon: "em breve",
    no_folder: "sem pasta",
    game_one: "jogo",
    games_n: "jogos",
    folder: "pasta",
    no_games: "nenhum jogo aqui",
    copy_to: "copie as ROMs para ",
    mapper: "mapper ",
    not_supported: " ainda nao suportado",
    needs: "precisa de ",
    inside: " em ",
    invalid: "arquivo invalido",
    open: "abrir",
    enter: "entrar",
    back: "voltar",
    favorite: "fav",
    settings_action: "ajustes",
    letter: "letra",
    ok: "ok",
    back_to_game: "voltar ao jogo",
    resume: "Continuar",
    reset: "Reset",
    save_state: "Salvar estado",
    load_state: "Carregar estado",
    shader: "Shader",
    quit_to_menu: "Voltar ao menu",
    colors: "Cores",
    background: "Fundo",
    lang: "Idioma",
    lang_name: "Portugues (BR)",
    video_mode: "Video",
    change: "mudar",
};

pub const EN: Texts = Texts {
    systems: "Systems",
    games: "Games",
    menu: "Menu",
    favorites: "Favorites",
    recent: "Recent",
    settings: "Settings",
    coming_soon: "coming soon",
    no_folder: "no folder",
    game_one: "game",
    games_n: "games",
    folder: "folder",
    no_games: "no games here",
    copy_to: "copy your ROMs to ",
    mapper: "mapper ",
    not_supported: " not supported yet",
    needs: "needs ",
    inside: " in ",
    invalid: "invalid file",
    open: "open",
    enter: "enter",
    back: "back",
    favorite: "fav",
    settings_action: "settings",
    letter: "letter",
    ok: "ok",
    back_to_game: "back to game",
    resume: "Resume",
    reset: "Reset",
    save_state: "Save state",
    load_state: "Load state",
    shader: "Shader",
    quit_to_menu: "Quit to menu",
    colors: "Colors",
    background: "Background",
    lang: "Language",
    lang_name: "English",
    video_mode: "Video",
    change: "change",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config() {
        assert_eq!(Language::from_config("EN"), Language::En);
        assert_eq!(Language::from_config(" pt-br "), Language::PtBr);
        assert_eq!(Language::from_config("pt-pt"), Language::PtBr);
        assert_eq!(Language::from_config(""), Language::PtBr);
    }
}
