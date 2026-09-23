//! Keep the tracking switch stable while the capture worker applies a request.

use std::sync::mpsc::Sender;

use crate::capture::UiState;
use crate::worker::WorkerCmd;

#[derive(Default)]
pub(super) struct TrackingControl {
    next_request_id: u64,
    pending: Option<Pending>,
}

#[derive(Clone, Copy)]
struct Pending {
    request_id: u64,
    desired: bool,
}

impl TrackingControl {
    pub(super) fn request_toggle(&mut self, commands: &Sender<WorkerCmd>, state: &mut UiState) {
        let desired = !state.tracking;
        let request_id = self.next_request_id + 1;
        if commands
            .send(WorkerCmd::SetTracking {
                on: desired,
                request_id: Some(request_id),
            })
            .is_ok()
        {
            self.next_request_id = request_id;
            self.pending = Some(Pending {
                request_id,
                desired,
            });
            state.tracking = desired;
        }
    }

    pub(super) fn reconcile(&mut self, snapshot: &mut UiState) {
        if let Some(pending) = self.pending {
            if snapshot.tracking_ack >= pending.request_id {
                self.pending = None;
            } else {
                snapshot.tracking = pending.desired;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(tracking: bool, tracking_ack: u64) -> UiState {
        UiState {
            tracking,
            tracking_ack,
            ..Default::default()
        }
    }

    #[test]
    fn stale_snapshot_does_not_revert_a_toggle() {
        let (commands, received) = std::sync::mpsc::channel();
        let mut control = TrackingControl::default();
        let mut displayed = state(true, 0);

        control.request_toggle(&commands, &mut displayed);
        assert!(!displayed.tracking);
        assert!(matches!(
            received.try_recv(),
            Ok(WorkerCmd::SetTracking {
                on: false,
                request_id: Some(1)
            })
        ));

        let mut stale = state(true, 0);
        control.reconcile(&mut stale);
        assert!(!stale.tracking);

        let mut confirmed = state(false, 1);
        control.reconcile(&mut confirmed);
        assert!(!confirmed.tracking);

        // A later tray action is visible after the UI request was confirmed.
        let mut later = state(true, 1);
        control.reconcile(&mut later);
        assert!(later.tracking);
    }

    #[test]
    fn rapid_double_toggle_waits_for_the_second_acknowledgment() {
        let (commands, received) = std::sync::mpsc::channel();
        let mut control = TrackingControl::default();
        let mut displayed = state(true, 0);

        control.request_toggle(&commands, &mut displayed);
        control.request_toggle(&commands, &mut displayed);
        assert!(displayed.tracking);
        assert!(matches!(
            received.try_recv(),
            Ok(WorkerCmd::SetTracking {
                on: false,
                request_id: Some(1)
            })
        ));
        assert!(matches!(
            received.try_recv(),
            Ok(WorkerCmd::SetTracking {
                on: true,
                request_id: Some(2)
            })
        ));

        let mut first_ack = state(false, 1);
        control.reconcile(&mut first_ack);
        assert!(first_ack.tracking);

        let mut second_ack = state(true, 2);
        control.reconcile(&mut second_ack);
        assert!(second_ack.tracking);

        let mut later = state(false, 2);
        control.reconcile(&mut later);
        assert!(!later.tracking);
    }
}
