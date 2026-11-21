use super::types::CancelDisposition;

pub fn derive_cancel_disposition(
    effect_outcome: Option<&str>,
    cancel_requested: bool,
    boundary_count: usize,
    in_flight: bool,
) -> CancelDisposition {
    if !cancel_requested {
        return CancelDisposition::None;
    }
    if in_flight {
        return CancelDisposition::RequestedInFlight;
    }
    let committed = boundary_count > 0;
    match effect_outcome {
        Some("completed") => CancelDisposition::SettledCompleted,
        Some("rolledBack") => CancelDisposition::SettledRolledBack,
        Some("partiallyApplied") => CancelDisposition::SettledPartially,
        Some("unknown") => CancelDisposition::SettledUnknown,
        _ => {
            if committed {
                CancelDisposition::SettledPartially
            } else {
                CancelDisposition::NotStarted
            }
        }
    }
}
