export type TriageId =
  | "severe-bleeding"
  | "hypothermia"
  | "shock"
  | "unconscious"
  | "poisoning";

export interface TriageChecklist {
  id: TriageId;
  title: string;
  summary: string;
  steps: string[];
  do: string[];
  doNot: string[];
  notes?: string;
}

export const TRIAGE_ITEMS: TriageChecklist[] = [
  {
    id: "severe-bleeding",
    title: "Severe bleeding",
    summary: "Heavy bleeding that soaks cloths or spurts. Act immediately to prevent shock.",
    steps: [
      "If safe, put on gloves or improvise a barrier (plastic bag, clean cloth).",
      "Have the person lie down. If there is no suspected head, neck, back, hip, or leg injury, elevate the bleeding limb above heart level.",
      "Apply firm, direct pressure with a clean dressing, cloth, or your hand. Do not remove soaked dressings; add more on top.",
      "Maintain continuous pressure for at least 10–15 minutes without checking under the dressing.",
      "If bleeding is from an arm or leg and does not slow with direct pressure, consider a tourniquet placed 5–7 cm above the wound (never on a joint). Tighten until bleeding stops.",
      "Continue to reassure the person, keep them warm, and be prepared to treat for shock.",
    ],
    do: [
      "Do use the cleanest available cloths or bandages you have.",
      "Do keep pressure steady; swap helpers rather than letting up.",
      "Do immobilize the injured part after bleeding is controlled.",
    ],
    doNot: [
      "Do not remove deeply embedded objects; apply pressure around them.",
      "Do not repeatedly lift the dressing to 'check' the wound.",
      "Do not apply a tourniquet directly over a joint or on a minor bleed.",
    ],
    notes: "If a tourniquet is used, note the time it was applied. Loosening and retightening repeatedly can worsen bleeding—only adjust if you must move it closer to the wound.",
  },
  {
    id: "hypothermia",
    title: "Hypothermia",
    summary: "Body core temperature dropping. Shivering, confusion, slurred speech, clumsiness.",
    steps: [
      "Move the person to a sheltered, dry area if possible. Block wind and get them off cold ground (pads, clothing, branches).",
      "Gently remove wet clothing and replace with dry layers. Cover the head, neck, armpits, and groin with insulation if available.",
      "Insulate from underneath with sleeping pads, blankets, or any dry material.",
      "Provide warm, sweet drinks if the person is awake, able to swallow, and not nauseated. Avoid alcohol.",
      "Rewarm the trunk (chest, armpits, back) first with shared body heat or warm bottles wrapped in cloth. Avoid direct hot contact with bare skin.",
      "Handle the person gently and keep movements slow to avoid triggering dangerous heart rhythms.",
    ],
    do: [
      "Do prioritize shelter from wind, wet, and ground.",
      "Do watch closely for worsening confusion, sleepiness, or loss of shivering.",
      "Do use multiple thin layers instead of one heavy blanket if possible.",
    ],
    doNot: [
      "Do not rub or massage cold limbs — this can damage tissues.",
      "Do not place hot water bottles directly on bare skin or over the limbs.",
      "Do not give food or drink if the person is not fully awake and able to swallow.",
    ],
    notes: "In moderate to severe hypothermia, focus on gentle handling and slow rewarming of the core. Sudden exertion can be dangerous.",
  },
  {
    id: "shock",
    title: "Shock",
    summary: "Circulation problem from blood loss, severe illness, allergic reaction, or trauma. Skin may be pale, cool, or clammy; pulse fast and weak.",
    steps: [
      "If the scene is safe, lay the person on their back. If there is no suspected head, neck, back, hip, or leg injury, consider raising the legs 20–30 cm.",
      "Loosen tight clothing around the neck, chest, and waist.",
      "Keep the person warm with blankets, coats, or improvised insulation.",
      "Control any obvious bleeding using firm direct pressure.",
      "Reassure the person, keep them still, and avoid unnecessary movement.",
      "Monitor breathing and responsiveness continuously. Be ready to start CPR if they stop breathing normally and you are trained.",
    ],
    do: [
      "Do treat underlying causes you can see (bleeding, cold, pain control within your skills).",
      "Do keep talking calmly and explain what you are doing.",
      "Do position the person for easiest breathing (slightly propped up) if they have chest injury or breathing difficulty.",
    ],
    doNot: [
      "Do not give food or drink if there is any chance they may need surgery or are vomiting.",
      "Do not let them walk around or exert themselves.",
      "Do not apply direct pressure to obvious broken bones piercing the skin; pad around them instead.",
    ],
    notes: "Shock is a sign of serious illness or injury. Even if the person feels better, keep monitoring them closely.",
  },
  {
    id: "unconscious",
    title: "Unconscious / unresponsive",
    summary: "Person does not respond normally to voice or touch.",
    steps: [
      "Check responsiveness: speak loudly, tap the shoulder, and ask if they are OK.",
      "If they do not respond, quickly check for normal breathing: look for chest rise, listen and feel for breath for up to 10 seconds.",
      "If they are not breathing normally and you are trained, begin CPR and send someone to call for emergency help if available.",
      "If they are breathing normally and you suspect no spine injury, place them in the recovery position on their side, with head tilted slightly back to keep the airway open.",
      "Clear obvious visible obstructions from the mouth if you can see them and they are easy to remove.",
      "Continue to monitor breathing and level of response until help is available or they wake up.",
    ],
    do: [
      "Do protect the airway first — position the head to keep the airway open.",
      "Do roll onto the side (recovery position) if vomiting or bleeding into the mouth is possible and spine injury seems unlikely.",
      "Do look for medical alert tags, pill bottles, or obvious causes (injury, overdose, seizure).",
    ],
    doNot: [
      "Do not shake the person or slap them to wake them.",
      "Do not put anything in their mouth, including food, drink, or pills.",
      "Do not move them if you suspect serious neck or back injury unless there is immediate danger (fire, collapsing structure, etc.).",
    ],
    notes: "If you are alone and must leave to get help, place the person in the recovery position first if you can do so safely.",
  },
  {
    id: "poisoning",
    title: "Poisoning / overdose",
    summary: "Exposure to toxic substances (pills, chemicals, gas, plants) or suspected overdose.",
    steps: [
      "Make the area safe: avoid breathing in fumes or touching spilled chemicals. Ventilate the space if possible without endangering yourself.",
      "Remove the person from the source if it is safe for you to do so.",
      "Check responsiveness and breathing. Treat as an unconscious person if they are unresponsive.",
      "If they are awake and able to swallow, ask what was taken, when, and how much. Save containers, pills, or plant samples for later reference.",
      "If poison was on the skin or clothing, remove contaminated clothing and rinse skin with plenty of clean water for at least 15 minutes.",
      "If poison was in the eyes, flush eyes with clean water for at least 15 minutes, keeping the affected eye lower so runoff does not spread.",
    ],
    do: [
      "Do move to fresh air immediately if fumes or gas are suspected.",
      "Do protect yourself with gloves or improvised barriers when handling substances or contaminated clothing.",
      "Do keep the person still and calm while you monitor breathing.",
    ],
    doNot: [
      "Do not induce vomiting unless a trained professional has explicitly instructed it for that specific poison.",
      "Do not give food, drink, or activated charcoal unless you are sure it is appropriate for the substance.",
      "Do not walk into an enclosed space with strong fumes without confirming it is safe.",
    ],
    notes: "Many poisons cause delayed symptoms. Even if the person appears well, continue to observe them closely, especially over the first few hours.",
  },
];

export function getTriageItem(id: TriageId): TriageChecklist | undefined {
  return TRIAGE_ITEMS.find((item) => item.id === id);
}

