//! Chrome internationalisation — the "language pack".
//!
//! UI text is looked up through a [`Strings`] table, one `const` per [`Lang`].
//! English is the source of truth; every other language fills the same fields, so
//! the compiler guarantees completeness (a new string can't ship half-translated).
//! Keycaps (`Ctrl+T`), symbols (◉ ⛭ 👓) and proper nouns (FOCUS, GAMBA, MCP,
//! claude/codex) are deliberately NOT translated — only the human-language
//! descriptions and section headers are.
//!
//! Translations here are authored in-house; a native polish pass is recommended
//! before a language is advertised as "supported".

use serde::{Deserialize, Serialize};

/// A UI language. Serialised lowercase into `state.toml` (`lang = "zh"`); an
/// unknown value falls back to English so old/foreign files always load.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Es,
    De,
    Zh,
}

impl Lang {
    /// Picker order.
    pub const ALL: [Lang; 4] = [Lang::En, Lang::Es, Lang::De, Lang::Zh];

    /// The language's own name, shown in the picker (autonym).
    pub fn native(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::De => "Deutsch",
            Lang::Zh => "中文",
        }
    }

    /// The next language in [`Self::ALL`] (wraps) — drives the click-to-cycle pill.
    pub fn next(self) -> Lang {
        let all = Self::ALL;
        let i = all.iter().position(|&l| l == self).unwrap_or(0);
        all[(i + 1) % all.len()]
    }

    /// This language's string table.
    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::En => &EN,
            Lang::Es => &ES,
            Lang::De => &DE,
            Lang::Zh => &ZH,
        }
    }
}

/// Every translatable chrome string, grouped by where it appears. Add a field
/// here and the compiler forces every language below to supply it.
pub struct Strings {
    // ── help modal · frame ──
    pub help: &'static str,
    pub shortcuts: &'static str,
    pub features: &'static str,
    pub demo_btn: &'static str,
    pub demo_sub: &'static str,
    pub help_footer: &'static str,
    // ── help modal · section headers ──
    pub s_tabs: &'static str,
    pub s_edit: &'static str,
    pub s_links: &'static str,
    pub s_scroll: &'static str,
    pub s_look: &'static str,
    pub s_agents: &'static str,
    pub s_window: &'static str,
    pub s_feat: &'static str,
    pub s_theming: &'static str,
    pub s_crt: &'static str,
    pub s_amcp: &'static str,
    // ── shortcuts view · descriptions ──
    pub new_tab: &'static str,
    pub switch_tabs: &'static str,
    pub move_tab: &'static str,
    pub split: &'static str,
    pub move_focus: &'static str,
    pub drag_subtab: &'static str,
    pub rclick_tab: &'static str,
    pub rclick: &'static str,
    pub copy_paste: &'static str,
    pub cut: &'static str,
    pub find: &'static str,
    pub find_all: &'static str,
    pub select_wl: &'static str,
    pub newline: &'static str,
    pub open_link: &'static str,
    pub scroll_hist: &'static str,
    pub clear_scroll: &'static str,
    pub themes_wheel: &'static str,
    pub display_tray: &'static str,
    pub text_size: &'static str,
    pub warp: &'static str,
    pub jump_msg: &'static str,
    pub nav_msg: &'static str,
    pub focus: &'static str,
    pub focus_inherit: &'static str,
    pub pan_focus: &'static str,
    pub input_colour: &'static str,
    pub bell: &'static str,
    pub mcp: &'static str,
    pub new_window: &'static str,
    // ── features view · descriptions ──
    pub f_tiling: &'static str,
    pub f_groups: &'static str,
    pub f_drag: &'static str,
    pub f_rename: &'static str,
    pub f_popout: &'static str,
    pub f_themes: &'static str,
    pub f_perpane: &'static str,
    pub f_wheel: &'static str,
    pub f_grade: &'static str,
    pub f_warp: &'static str,
    pub f_scan: &'static str,
    pub f_crawl: &'static str,
    pub f_gamba: &'static str,
    pub f_detect: &'static str,
    pub f_focus: &'static str,
    pub f_restore: &'static str,
    pub f_mcp: &'static str,
}

/// English — the source of truth. Strings match the original inline literals.
pub const EN: Strings = Strings {
    help: "HELP",
    shortcuts: "SHORTCUTS",
    features: "FEATURES",
    demo_btn: "Spin up a demo of this layout",
    demo_sub: "opens a new window cloning this exact layout, every pane filled with lorem-ipsum — safe to screen-share",
    help_footer: "Esc or click outside to close · themes are live-editable TOML while it runs",
    s_tabs: "TABS & PANES",
    s_edit: "EDITING & CLIPBOARD",
    s_links: "LINKS",
    s_scroll: "SCROLLBACK",
    s_look: "LOOK & FEEL",
    s_agents: "AGENTS · claude / codex",
    s_window: "WINDOW",
    s_feat: "PANES · TABS · WINDOW",
    s_theming: "THEMING",
    s_crt: "CRT · MOTION",
    s_amcp: "AGENTS · MCP",
    new_tab: "New tab",
    switch_tabs: "Switch tabs",
    move_tab: "Move tab (in / across groups)",
    split: "Split ↔ / ↕",
    move_focus: "Move focus between panes",
    drag_subtab: "Move / split · drag out = new window",
    rclick_tab: "Rename · colour · group",
    rclick: "Copy · Paste · Open link · Clear",
    copy_paste: "Copy / Paste",
    cut: "Cut selection (deletes on the input line)",
    find: "Find in this pane (fuzzy) — ↵ jumps to it",
    find_all: "Find across ALL panes (fuzzy)",
    select_wl: "Select word / line",
    newline: "Newline (multiline in claude/codex)",
    open_link: "Open a URL or file path",
    scroll_hist: "Scroll history",
    clear_scroll: "Clear scrollback (not Ctrl+L)",
    themes_wheel: "Themes & colour wheel",
    display_tray: "Monitor grade · text size · text-crawl",
    text_size: "Text size",
    warp: "Curve the glass (0 = flat → fishbowl)",
    jump_msg: "Jump to your previous / next message",
    nav_msg: "Same — navigate your own messages",
    focus: "FOCUS — mirror this pane big",
    focus_inherit: "Reader curves + glares like its pane",
    pan_focus: "Pan a zoomed FOCUS read down / sideways",
    input_colour: "Your turns stand out (👤 wheel pip)",
    bell: "Pane shows ● done + a per-agent sound",
    mcp: "MCP — read-only agent-watch surface",
    new_window: "New window (quick scratch)",
    f_tiling: "Splits divide only the focused pane",
    f_groups: "Colour band · handle chip · collapse pill",
    f_drag: "Split, move, or tear off a new window",
    f_rename: "Caret, selection, word-nav on tab names",
    f_popout: "Quick window · single-instance aware",
    f_themes: "Live-editable TOML, ~300ms hot reload",
    f_perpane: "Theme & grade inherit · 'follow outer'",
    f_wheel: "3 markers — seed · text · complement",
    f_grade: "Bright/contrast/colour/text/bg/gamma + size",
    f_warp: "GPU curve · 0→1.5 dial · per-pane",
    f_scan: "Vignette · phosphor · tracking · jiggle",
    f_crawl: "Star-Wars perspective recede (angle · depth)",
    f_gamba: "Slot machine spins while an agent thinks",
    f_detect: "Detected · jump messages · done bell",
    f_focus: "Mirror a pane big over a frosted blur",
    f_restore: "Crash / quit → resume the exact chat",
    f_mcp: "Read-only watch + push · stdio, never TCP",
};

/// Español.
pub const ES: Strings = Strings {
    help: "AYUDA",
    shortcuts: "ATAJOS",
    features: "FUNCIONES",
    demo_btn: "Abrir una demo de esta disposición",
    demo_sub: "abre una ventana nueva que clona esta disposición, cada panel lleno de texto de relleno — seguro para compartir pantalla",
    help_footer: "Esc o clic fuera para cerrar · los temas son TOML editable en vivo mientras corre",
    s_tabs: "PESTAÑAS Y PANELES",
    s_edit: "EDICIÓN Y PORTAPAPELES",
    s_links: "ENLACES",
    s_scroll: "HISTORIAL",
    s_look: "ASPECTO",
    s_agents: "AGENTES · claude / codex",
    s_window: "VENTANA",
    s_feat: "PANELES · PESTAÑAS · VENTANA",
    s_theming: "TEMAS",
    s_crt: "CRT · MOVIMIENTO",
    s_amcp: "AGENTES · MCP",
    new_tab: "Nueva pestaña",
    switch_tabs: "Cambiar de pestaña",
    move_tab: "Mover pestaña (dentro / entre grupos)",
    split: "Dividir ↔ / ↕",
    move_focus: "Mover el foco entre paneles",
    drag_subtab: "Mover / dividir · arrastrar fuera = ventana nueva",
    rclick_tab: "Renombrar · color · grupo",
    rclick: "Copiar · Pegar · Abrir enlace · Limpiar",
    copy_paste: "Copiar / Pegar",
    cut: "Cortar selección (borra en la línea de entrada)",
    find: "Buscar en este panel (difuso) — ↵ salta a él",
    find_all: "Buscar en TODOS los paneles (difuso)",
    select_wl: "Seleccionar palabra / línea",
    newline: "Salto de línea (multilínea en claude/codex)",
    open_link: "Abrir una URL o ruta de archivo",
    scroll_hist: "Desplazar el historial",
    clear_scroll: "Limpiar el historial (no Ctrl+L)",
    themes_wheel: "Temas y rueda de color",
    display_tray: "Ajuste del monitor · tamaño de texto · text-crawl",
    text_size: "Tamaño del texto",
    warp: "Curva el cristal (0 = plano → pecera)",
    jump_msg: "Salta a tu mensaje anterior / siguiente",
    nav_msg: "Igual — navega tus propios mensajes",
    focus: "FOCUS — refleja este panel en grande",
    focus_inherit: "El lector se curva y brilla como su panel",
    pan_focus: "Desplaza una lectura FOCUS ampliada abajo / a los lados",
    input_colour: "Tus turnos resaltan (pip 👤 de la rueda)",
    bell: "El panel muestra ● listo + un sonido por agente",
    mcp: "MCP — superficie de observación de agentes, solo lectura",
    new_window: "Nueva ventana (borrador rápido)",
    f_tiling: "Las divisiones afectan solo al panel enfocado",
    f_groups: "Banda de color · ficha de asa · píldora de colapso",
    f_drag: "Divide, mueve o desprende una ventana nueva",
    f_rename: "Cursor, selección y navegación por palabras en los nombres",
    f_popout: "Ventana rápida · consciente de instancia única",
    f_themes: "TOML editable en vivo, recarga en ~300ms",
    f_perpane: "Tema y ajuste se heredan · «seguir exterior»",
    f_wheel: "3 marcadores — semilla · texto · complemento",
    f_grade: "Brillo/contraste/color/texto/fondo/gamma + tamaño",
    f_warp: "Curva por GPU · dial 0→1.5 · por panel",
    f_scan: "Viñeta · fósforo · tracking · vibración",
    f_crawl: "Perspectiva tipo Star Wars que se aleja (ángulo · profundidad)",
    f_gamba: "Una tragaperras gira mientras un agente piensa",
    f_detect: "Detectado · salta mensajes · timbre al terminar",
    f_focus: "Refleja un panel en grande sobre un desenfoque esmerilado",
    f_restore: "Caída / cierre → retoma el chat exacto",
    f_mcp: "Observación solo lectura + push · stdio, nunca TCP",
};

/// Deutsch.
pub const DE: Strings = Strings {
    help: "HILFE",
    shortcuts: "TASTENKÜRZEL",
    features: "FUNKTIONEN",
    demo_btn: "Eine Demo dieses Layouts starten",
    demo_sub: "öffnet ein neues Fenster, das genau dieses Layout klont, jedes Panel mit Blindtext gefüllt — sicher zum Teilen des Bildschirms",
    help_footer: "Esc oder Klick außerhalb zum Schließen · Themes sind live editierbares TOML zur Laufzeit",
    s_tabs: "TABS & PANELS",
    s_edit: "BEARBEITEN & ZWISCHENABLAGE",
    s_links: "LINKS",
    s_scroll: "VERLAUF",
    s_look: "ERSCHEINUNGSBILD",
    s_agents: "AGENTEN · claude / codex",
    s_window: "FENSTER",
    s_feat: "PANELS · TABS · FENSTER",
    s_theming: "THEMES",
    s_crt: "CRT · BEWEGUNG",
    s_amcp: "AGENTEN · MCP",
    new_tab: "Neuer Tab",
    switch_tabs: "Tabs wechseln",
    move_tab: "Tab verschieben (in / zwischen Gruppen)",
    split: "Teilen ↔ / ↕",
    move_focus: "Fokus zwischen Panels bewegen",
    drag_subtab: "Verschieben / teilen · herausziehen = neues Fenster",
    rclick_tab: "Umbenennen · Farbe · Gruppe",
    rclick: "Kopieren · Einfügen · Link öffnen · Leeren",
    copy_paste: "Kopieren / Einfügen",
    cut: "Auswahl ausschneiden (löscht in der Eingabezeile)",
    find: "In diesem Panel suchen (unscharf) — ↵ springt hin",
    find_all: "In ALLEN Panels suchen (unscharf)",
    select_wl: "Wort / Zeile auswählen",
    newline: "Zeilenumbruch (mehrzeilig in claude/codex)",
    open_link: "Eine URL oder einen Dateipfad öffnen",
    scroll_hist: "Verlauf scrollen",
    clear_scroll: "Verlauf leeren (nicht Ctrl+L)",
    themes_wheel: "Themes & Farbrad",
    display_tray: "Monitor-Abstimmung · Textgröße · Text-Crawl",
    text_size: "Textgröße",
    warp: "Das Glas wölben (0 = flach → Fischglas)",
    jump_msg: "Zur vorherigen / nächsten Nachricht springen",
    nav_msg: "Dasselbe — durch eigene Nachrichten navigieren",
    focus: "FOCUS — dieses Panel groß spiegeln",
    focus_inherit: "Der Leser wölbt und glänzt wie sein Panel",
    pan_focus: "Eine gezoomte FOCUS-Ansicht nach unten / seitlich schwenken",
    input_colour: "Deine Beiträge stechen hervor (👤-Rad-Pip)",
    bell: "Panel zeigt ● fertig + einen Ton pro Agent",
    mcp: "MCP — schreibgeschützte Agenten-Beobachtung",
    new_window: "Neues Fenster (schneller Notizblock)",
    f_tiling: "Teilungen betreffen nur das fokussierte Panel",
    f_groups: "Farbband · Griff-Chip · Einklapp-Pille",
    f_drag: "Teilen, verschieben oder als neues Fenster abtrennen",
    f_rename: "Cursor, Auswahl, Wortnavigation in Tab-Namen",
    f_popout: "Schnelles Fenster · einzel-instanz-bewusst",
    f_themes: "Live editierbares TOML, ~300ms Hot-Reload",
    f_perpane: "Theme & Abstimmung werden vererbt · „äußerem folgen“",
    f_wheel: "3 Marker — Saat · Text · Komplement",
    f_grade: "Helligkeit/Kontrast/Farbe/Text/Hintergrund/Gamma + Größe",
    f_warp: "GPU-Wölbung · Regler 0→1.5 · pro Panel",
    f_scan: "Vignette · Phosphor · Tracking · Zittern",
    f_crawl: "Star-Wars-Perspektive, die zurückweicht (Winkel · Tiefe)",
    f_gamba: "Ein Spielautomat dreht, während ein Agent nachdenkt",
    f_detect: "Erkannt · Nachrichten überspringen · Fertig-Glocke",
    f_focus: "Ein Panel groß über einer Mattglas-Unschärfe spiegeln",
    f_restore: "Absturz / Beenden → genau den Chat wiederaufnehmen",
    f_mcp: "Schreibgeschützte Beobachtung + Push · stdio, nie TCP",
};

/// 中文 (简体).
pub const ZH: Strings = Strings {
    help: "帮助",
    shortcuts: "快捷键",
    features: "功能",
    demo_btn: "启动此布局的演示",
    demo_sub: "打开一个克隆当前布局的新窗口，每个面板都填充示例文本——可安全用于共享屏幕",
    help_footer: "按 Esc 或点击外部关闭 · 运行时主题是可实时编辑的 TOML",
    s_tabs: "标签页与面板",
    s_edit: "编辑与剪贴板",
    s_links: "链接",
    s_scroll: "回滚",
    s_look: "外观",
    s_agents: "智能体 · claude / codex",
    s_window: "窗口",
    s_feat: "面板 · 标签页 · 窗口",
    s_theming: "主题",
    s_crt: "CRT · 动效",
    s_amcp: "智能体 · MCP",
    new_tab: "新建标签页",
    switch_tabs: "切换标签页",
    move_tab: "移动标签页（组内 / 跨组）",
    split: "拆分 ↔ / ↕",
    move_focus: "在面板间移动焦点",
    drag_subtab: "移动 / 拆分 · 拖出 = 新窗口",
    rclick_tab: "重命名 · 颜色 · 分组",
    rclick: "复制 · 粘贴 · 打开链接 · 清屏",
    copy_paste: "复制 / 粘贴",
    cut: "剪切所选（在输入行上会删除）",
    find: "在此面板中查找（模糊）——↵ 跳转",
    find_all: "在所有面板中查找（模糊）",
    select_wl: "选择单词 / 整行",
    newline: "换行（在 claude/codex 中可多行）",
    open_link: "打开网址或文件路径",
    scroll_hist: "滚动历史",
    clear_scroll: "清除回滚（非 Ctrl+L）",
    themes_wheel: "主题与色环",
    display_tray: "显示器调校 · 文字大小 · 文字滚屏",
    text_size: "文字大小",
    warp: "弯曲屏幕（0 = 平面 → 鱼缸）",
    jump_msg: "跳到你的上一条 / 下一条消息",
    nav_msg: "同上——浏览你自己的消息",
    focus: "FOCUS——放大镜像此面板",
    focus_inherit: "阅读器像其面板一样弯曲并泛光",
    pan_focus: "平移放大的 FOCUS 阅读：向下 / 左右",
    input_colour: "你的发言更突出（👤 色环标记）",
    bell: "面板显示 ● 完成 + 每个智能体的提示音",
    mcp: "MCP——只读的智能体观察面",
    new_window: "新窗口（快速草稿）",
    f_tiling: "拆分只切分当前聚焦的面板",
    f_groups: "色带 · 手柄标签 · 折叠胶囊",
    f_drag: "拆分、移动或撕出为新窗口",
    f_rename: "标签名支持光标、选择与按词导航",
    f_popout: "快速窗口 · 感知单实例",
    f_themes: "可实时编辑的 TOML，约 300 毫秒热重载",
    f_perpane: "主题与调校可继承 · “跟随外层”",
    f_wheel: "3 个标记——种子 · 文字 · 补色",
    f_grade: "亮度/对比度/颜色/文字/背景/伽马 + 大小",
    f_warp: "GPU 曲面 · 0→1.5 旋钮 · 逐面板",
    f_scan: "暗角 · 荧光 · 行扫 · 抖动",
    f_crawl: "《星球大战》式向远处后退的透视（角度 · 深度）",
    f_gamba: "智能体思考时老虎机转动",
    f_detect: "已检测 · 跳转消息 · 完成提示音",
    f_focus: "在磨砂模糊上放大镜像一个面板",
    f_restore: "崩溃 / 退出 → 恢复到完全相同的会话",
    f_mcp: "只读观察 + 推送 · stdio，绝不用 TCP",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_resolves_and_cycles() {
        // ALL is non-empty, next() cycles through it and returns home.
        let mut l = Lang::default();
        for _ in 0..Lang::ALL.len() {
            // each language has a non-empty autonym, chip, and resolves a table
            assert!(!l.native().is_empty());
            let _ = l.strings();
            l = l.next();
        }
        assert_eq!(l, Lang::default(), "next() must cycle back to the start");
    }

    #[test]
    fn english_is_the_source_strings() {
        // a couple of anchors so a careless edit to EN is caught
        assert_eq!(Lang::En.strings().new_tab, "New tab");
        assert_eq!(Lang::En.strings().help, "HELP");
    }
}
