//! Words for other machines: the window that shows another machine's
//! Terminal Delight, the Machines page in Settings, and far agents on the
//! Threshing Floor.
//!
//! Written in-house with the rest of the table's conventions: keycaps, `ssh`,
//! file names and product names stay as they are, and every `{placeholder}`
//! survives. English is the source of truth.

pub struct Far {
    /// The banner when a far machine answers again. {machine} is its name in capitals.
    pub back: &'static str,
    /// The strip across a far window while its machine does not answer.
    pub down: &'static str,
    /// Under it: what became of the terminals. They are not ended.
    pub down_held: &'static str,
    /// When the far machine has started a new session host. {machine} in capitals.
    pub replaced: &'static str,
    /// Under it. What this window shows is no longer what runs there.
    pub replaced_sub: &'static str,
    /// Tooltip on the chip that names the machine a far window shows. {machine} as written in machines.toml, {dest} is the ssh destination.
    pub chip_tip: &'static str,
    /// On a pane that the far machine's own window is showing instead. {machine} in capitals. Keep the keycap.
    pub taken: &'static str,
}

pub const EN: Far = Far {
    back: "{machine} · the connection is back",
    down: "{machine} · connection lost, trying again",
    down_held: "Its terminals are still running there. They are held here as they were.",
    replaced: "{machine} · its session was restarted",
    replaced_sub:
        "What this window shows is no longer running there. Close it and open the machine again.",
    chip_tip: "This window shows Terminal Delight on {machine} ({dest}). Its terminals run there.",
    taken: "Shown on {machine}'s own window · ↵ brings it here",
};

pub const ES: Far = Far {
    back: "{machine} · la conexión ha vuelto",
    down: "{machine} · conexión perdida, reintentando",
    down_held: "Sus terminales siguen funcionando allí. Aquí se conservan como estaban.",
    replaced: "{machine} · su sesión se reinició",
    replaced_sub: "Lo que muestra esta ventana ya no se ejecuta allí. Ciérrala y abre la máquina de nuevo.",
    chip_tip: "Esta ventana muestra Terminal Delight en {machine} ({dest}). Sus terminales se ejecutan allí.",
    taken: "Se muestra en la ventana de {machine} · ↵ lo trae aquí",
};

pub const DE: Far = Far {
    back: "{machine} · die Verbindung steht wieder",
    down: "{machine} · Verbindung verloren, neuer Versuch",
    down_held: "Die Terminals laufen dort weiter. Hier bleiben sie stehen, wie sie waren.",
    replaced: "{machine} · die Sitzung dort wurde neu gestartet",
    replaced_sub: "Was dieses Fenster zeigt, läuft dort nicht mehr. Schließe es und öffne die Maschine erneut.",
    chip_tip: "Dieses Fenster zeigt Terminal Delight auf {machine} ({dest}). Die Terminals laufen dort.",
    taken: "Wird im eigenen Fenster von {machine} gezeigt · ↵ holt es hierher",
};

pub const FR: Far = Far {
    back: "{machine} · la connexion est revenue",
    down: "{machine} · connexion perdue, nouvel essai",
    down_held: "Ses terminaux tournent toujours là-bas. Ils restent ici tels qu'ils étaient.",
    replaced: "{machine} · sa session a été redémarrée",
    replaced_sub: "Ce que montre cette fenêtre ne tourne plus là-bas. Fermez-la et rouvrez la machine.",
    chip_tip: "Cette fenêtre montre Terminal Delight sur {machine} ({dest}). Ses terminaux tournent là-bas.",
    taken: "Affiché dans la fenêtre de {machine} · ↵ le ramène ici",
};

pub const RU: Far = Far {
    back: "{machine} · соединение восстановлено",
    down: "{machine} · соединение потеряно, пробуем снова",
    down_held: "Его терминалы продолжают работать там. Здесь они показаны как были.",
    replaced: "{machine} · сеанс там был перезапущен",
    replaced_sub: "То, что показано в этом окне, там больше не работает. Закройте его и откройте машину снова.",
    chip_tip: "Это окно показывает Terminal Delight на {machine} ({dest}). Терминалы работают там.",
    taken: "Показано в собственном окне {machine} · ↵ переносит сюда",
};

pub const ZH: Far = Far {
    back: "{machine} · 连接已恢复",
    down: "{machine} · 连接已断开，正在重试",
    down_held: "它的终端仍在那边运行，这里保持原样显示。",
    replaced: "{machine} · 那边的会话已重新启动",
    replaced_sub: "此窗口显示的内容已不在那边运行。请关闭它并重新打开该机器。",
    chip_tip: "此窗口显示 {machine}（{dest}）上的 Terminal Delight，终端在那边运行。",
    taken: "正在 {machine} 自己的窗口中显示 · ↵ 移到这里",
};

pub const JA: Far = Far {
    back: "{machine} · 接続が戻りました",
    down: "{machine} · 接続が切れました。再試行中",
    down_held: "端末は向こうで動き続けています。ここでは元の表示のまま保たれます。",
    replaced: "{machine} · セッションが再起動されました",
    replaced_sub: "このウィンドウの内容はもう向こうでは動いていません。閉じてマシンを開き直してください。",
    chip_tip: "このウィンドウは {machine}（{dest}）の Terminal Delight を表示しています。端末は向こうで動いています。",
    taken: "{machine} 側のウィンドウで表示中 · ↵ でここに戻す",
};

pub const KO: Far = Far {
    back: "{machine} · 연결이 돌아왔습니다",
    down: "{machine} · 연결이 끊겼습니다. 다시 시도하는 중",
    down_held: "터미널은 그쪽에서 계속 실행 중입니다. 여기서는 그대로 유지됩니다.",
    replaced: "{machine} · 세션이 다시 시작되었습니다",
    replaced_sub:
        "이 창에 보이는 것은 더 이상 그쪽에서 실행되지 않습니다. 창을 닫고 기기를 다시 여세요.",
    chip_tip:
        "이 창은 {machine}({dest})의 Terminal Delight를 보여 줍니다. 터미널은 그쪽에서 실행됩니다.",
    taken: "{machine}의 창에서 표시 중 · ↵ 여기로 가져오기",
};

pub const HI: Far = Far {
    back: "{machine} · कनेक्शन वापस आ गया",
    down: "{machine} · कनेक्शन टूट गया, फिर कोशिश हो रही है",
    down_held: "उसके टर्मिनल वहाँ चल रहे हैं। यहाँ वे वैसे ही रखे गए हैं।",
    replaced: "{machine} · उसका सत्र फिर से शुरू हुआ",
    replaced_sub: "इस विंडो में जो दिख रहा है वह अब वहाँ नहीं चल रहा। इसे बंद करें और मशीन फिर खोलें।",
    chip_tip: "यह विंडो {machine} ({dest}) का Terminal Delight दिखाती है। उसके टर्मिनल वहीं चलते हैं।",
    taken: "{machine} की अपनी विंडो में दिख रहा है · ↵ इसे यहाँ लाता है",
};
