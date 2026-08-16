export type MembershipCheckState = "on" | "off" | "mixed";

/** on / off / mixed from how many of the selected rows already have each name. */
export function checksFromMembers(
  names: string[],
  memberLists: readonly string[][],
): Record<string, MembershipCheckState> {
  const checks: Record<string, MembershipCheckState> = {};
  for (const name of names) {
    const hits = memberLists.filter((list) =>
      list.some((item) => item.toLowerCase() === name.toLowerCase()),
    ).length;
    if (hits === 0) checks[name] = "off";
    else if (memberLists.length > 0 && hits === memberLists.length) {
      checks[name] = "on";
    } else checks[name] = "mixed";
  }
  return checks;
}
