//! Words for High trust agentic: the setting that lets an agent a person
//! talks to start a team of other agents, and the banner that says one
//! did.
//!
//! Written in-house with the rest of the table's conventions: keycaps, `ssh`,
//! file names and product names stay as they are, and every `{placeholder}`
//! survives. English is the source of truth.

pub struct Team {
    /// Settings row title: the switch that lets an agent start other agents. A name; keep it short.
    pub st_title: &'static str,
    /// Under it: what switching it on allows, and the one limit.
    pub st_sub: &'static str,
    /// The banner when an agent starts a team. {who} is the coordinator's pane name, {team} the team's name, {count} how many agents.
    pub started: &'static str,
    /// Row in a seat's right-click menu on the Threshing Floor: promote this pane so its agent may start other agents. A filled or hollow dot is drawn before it.
    pub orch_row: &'static str,
    /// Appended to that row while the window's High trust agentic switch is off, so the promotion does nothing yet.
    pub orch_needs_switch: &'static str,
}

pub const EN: Team = Team {
    st_title: "High trust agentic",
    st_sub: "An agent you promote to orchestrator may open a tab, start other agents in it and prompt them. Promote a pane from its card on the Threshing Floor. Off: only you start agents.",
    started: "{who} started a team: {team} · {count} agents",
    orch_row: "Orchestrator: may start agents",
    orch_needs_switch: "needs High trust agentic",
};

pub const ES: Team = Team {
    st_title: "Agentes de alta confianza",
    st_sub: "Un agente que promuevas a orquestador puede abrir una pestaña, iniciar otros agentes en ella y darles indicaciones. Promueve un panel desde su tarjeta en el Threshing Floor. Apagado: solo tú inicias agentes.",
    started: "{who} inició un equipo: {team} · {count} agentes",
    orch_row: "Orquestador: puede iniciar agentes",
    orch_needs_switch: "requiere Agentes de alta confianza",
};

pub const DE: Team = Team {
    st_title: "Agenten mit hohem Vertrauen",
    st_sub: "Ein Agent, den du zum Orchestrator beförderst, darf einen Tab öffnen, darin weitere Agenten starten und sie beauftragen. Befördere einen Bereich über seine Karte im Threshing Floor. Aus: nur du startest Agenten.",
    started: "{who} hat ein Team gestartet: {team} · {count} Agenten",
    orch_row: "Orchestrator: darf Agenten starten",
    orch_needs_switch: "braucht Agenten mit hohem Vertrauen",
};

pub const FR: Team = Team {
    st_title: "Agents à haute confiance",
    st_sub: "Un agent que vous promouvez orchestrateur peut ouvrir un onglet, y démarrer d'autres agents et leur donner des instructions. Promouvez un panneau depuis sa carte sur le Threshing Floor. Désactivé : vous seul démarrez des agents.",
    started: "{who} a démarré une équipe : {team} · {count} agents",
    orch_row: "Orchestrateur : peut démarrer des agents",
    orch_needs_switch: "nécessite Agents à haute confiance",
};

pub const RU: Team = Team {
    st_title: "Агенты с высоким доверием",
    st_sub: "Агент, которого вы назначили оркестратором, может открыть вкладку, запустить в ней других агентов и давать им поручения. Назначьте панель через её карточку на Threshing Floor. Выключено: агентов запускаете только вы.",
    started: "{who} запустил команду: {team} · агентов: {count}",
    orch_row: "Оркестратор: может запускать агентов",
    orch_needs_switch: "нужен режим «Агенты с высоким доверием»",
};

pub const ZH: Team = Team {
    st_title: "高信任代理模式",
    st_sub: "你提升为编排者的代理可以打开标签页、在其中启动其他代理并向它们下达指令。在 Threshing Floor 上通过窗格的卡片进行提升。关闭时：只有你能启动代理。",
    started: "{who} 启动了团队：{team} · {count} 个代理",
    orch_row: "编排者：可以启动代理",
    orch_needs_switch: "需要开启高信任代理模式",
};

pub const JA: Team = Team {
    st_title: "高信頼エージェント",
    st_sub: "オーケストレーターに昇格させたエージェントは、タブを開き、そこで別のエージェントを起動して指示できます。昇格は Threshing Floor のカードから行います。オフ: エージェントを起動するのはあなただけです。",
    started: "{who} がチームを開始: {team} · エージェント {count}",
    orch_row: "オーケストレーター: エージェントを起動できる",
    orch_needs_switch: "高信頼エージェントが必要",
};

pub const KO: Team = Team {
    st_title: "고신뢰 에이전트",
    st_sub: "오케스트레이터로 승격한 에이전트는 탭을 열고 그 안에서 다른 에이전트를 시작해 지시할 수 있습니다. Threshing Floor의 카드에서 창을 승격하세요. 끔: 에이전트는 사용자만 시작합니다.",
    started: "{who}이(가) 팀을 시작함: {team} · 에이전트 {count}",
    orch_row: "오케스트레이터: 에이전트를 시작할 수 있음",
    orch_needs_switch: "고신뢰 에이전트 필요",
};

pub const HI: Team = Team {
    st_title: "उच्च भरोसे वाले एजेंट",
    st_sub: "जिस एजेंट को आप ऑर्केस्ट्रेटर बनाते हैं वह एक टैब खोल सकता है, उसमें दूसरे एजेंट शुरू कर सकता है और उन्हें निर्देश दे सकता है। Threshing Floor पर पैन के कार्ड से उसे ऑर्केस्ट्रेटर बनाएँ। बंद: एजेंट केवल आप शुरू करते हैं।",
    started: "{who} ने टीम शुरू की: {team} · {count} एजेंट",
    orch_row: "ऑर्केस्ट्रेटर: एजेंट शुरू कर सकता है",
    orch_needs_switch: "उच्च भरोसे वाले एजेंट चालू होना चाहिए",
};
