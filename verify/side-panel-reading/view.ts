// The words a person reads on the side-panel reading chart (pstack /statechart).
export const title = "One side-panel tab, read and read again";

export const state = (v: any) => {
  const again = v.out ? ", being read again" : "";
  if (v.got && v.error) return `The answer, with a red note${again}`;
  if (v.got) return `The answer${again}`;
  if (v.error) return v.out ? "A red note, being tried again" : "A red note, nothing to show";
  return v.out ? "Reading…" : "Not read yet";
};

export const group = (v: any) => (v.error ? "Trouble" : v.got ? "Shown" : "Waiting");

export const colors = { Waiting: "yellow", Shown: "teal", Trouble: "tan" };

export const step = (action: string, from: any, to: any) => {
  switch (action) {
    case "YouOrThePollRead":
      return to.forced ? "you open it or press r" : "the poll reads it";
    case "ReadLands":
      return "an answer lands";
    case "NothingMoved":
      return "the poll finds nothing moved";
    case "ReadFails":
      return "the read fails";
    // A reset every state has (a moved pull request drops its diff; a closed
    // tab drops its reading): hidden, and the skill says so.
    case "TheDiffIsDropped":
      return null;
    // The world changing moves no label, so these are never drawn as arrows;
    // they are named for the bug paths in the report.
    case "ItGoesAway":
      return "it goes away (moved, deleted, gh failing)";
    case "ItComesBack":
      return to.moved ? "it comes back changed" : "it comes back as it was";
    case "SomeoneEditsIt":
      return "someone edits it";
  }
  return action;
};
