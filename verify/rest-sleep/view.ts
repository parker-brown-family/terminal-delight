// The words a person reads on the rest-and-sleep chart. Used with the pstack
// /statechart skill: states are what the pane looks like to the person,
// arrows are what happens, as it happens.
const agentWord: Record<string, string> = {
  prompt: "at its prompt",
  working: "working",
  waiting: "waiting on its own timer",
};

export const title = "An agent pane resting and sleeping";

export const state = (v: any) => {
  const a = agentWord[v.agent];
  switch (v.stage) {
    case "awake":
      return `Awake, ${a}`;
    case "looked":
      return `About to rest, ${a}`;
    case "resting":
      return `Frozen, ${a}`;
    case "asleep":
      return v.lost ? "Asleep, its loop lost" : "Asleep";
  }
  return v.stage;
};

export const group = (v: any) =>
  ({ awake: "Awake", looked: "Awake", resting: "Resting", asleep: "Asleep" } as any)[v.stage];

export const colors = { Awake: "teal", Resting: "blue", Asleep: "purple" };

export const step = (action: string, from: any, to: any) => {
  switch (action) {
    case "PersonTypes":
      return from.stage === "asleep"
        ? "you press a key; it wakes"
        : from.stage === "resting"
          ? "you press a key; it thaws"
          : "you type to it";
    case "AgentFinishes":
      return "it finishes its turn";
    case "AgentSchedulesWakeup":
      return "it ends a turn with a timer";
    case "WakeupFires":
      return "its timer fires";
    case "Look":
      return "the window looks: idle";
    case "Freeze":
      return to.stage === "resting" ? "the window freezes it" : "the window sees your key, stays awake";
    case "Sleep":
      return "the window ends it, keeping the conversation";
  }
  return action;
};
