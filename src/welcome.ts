/**
 * When the Dashboard shows its first-run welcome card: until the user has
 * completed a session (none in this run, none in `emissions.csv`) and as
 * long as they have not closed it. Hidden while either is still unknown,
 * so it never flashes for someone who has used the app before.
 */
export interface WelcomeInputs {
  /** `settings.welcome_dismissed`; null until the settings are loaded. */
  dismissed: boolean | null;
  /** A session stopped since the app started (`state.last_summary`). */
  hasLastSummary: boolean;
  /** `get_first_run_info().has_sessions`; null until it answered. */
  hasSessions: boolean | null;
}

export function showWelcome({ dismissed, hasLastSummary, hasSessions }: WelcomeInputs): boolean {
  return dismissed === false && !hasLastSummary && hasSessions === false;
}
