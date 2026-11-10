// The words a person reads on the composer chart (pstack /statechart).
export const title = "The composer: the sentence, a chip's list, and the dialog over them";

export const state = (v: any) => {
  if (!v.up) return "Put away";
  switch (v.face) {
    case "sentence":
      return "On the sentence";
    case "project": {
      // `order` is the stored list the code before drew; under Derived the list is
      // worked out from what is typed, so read this chart for the before config.
      const shows = v.order === "all" ? "every project" : "the matches";
      return v.query === "" ? `PROJECT's list, nothing typed, showing ${shows}` : `PROJECT's list, letters typed, showing ${shows}`;
    }
    case "other":
      return v.query === "" ? "Another chip's list" : "Another chip's list, letters typed";
    case "adding":
      return "Adding a project";
  }
  return v.face;
};

export const group = (v: any) => (!v.up ? "Away" : v.face === "adding" ? "Dialog" : "Composer");

export const colors = { Away: "tan", Composer: "teal", Dialog: "yellow" };

export const step = (action: string, from: any, to: any) => {
  switch (action) {
    case "YouOpenIt":
      return "you open it";
    case "YouMoveBetweenFaces":
      return "you Tab, press a chip, or Esc or Enter off one";
    case "YouTypeOnAChip":
      return "you type";
    case "YouBackspaceItAway":
      return "you backspace to nothing";
    case "YouPressTheSentence":
      return "you press the sentence";
    case "YouAddAProject":
      return "you press Ctrl+O";
    case "YouGoBackFromTheDialog":
      return "you go back from the dialog";
    case "TheFolderIsTaken":
      return "a folder is taken";
    case "YouPutItAway":
      return "you put it away";
  }
  return action;
};
