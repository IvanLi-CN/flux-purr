use super::*;
use std::collections::VecDeque;

#[test]
fn unread_usb_response_does_not_starve_lan_control_mailbox() {
    struct Transports {
        mailbox: VecDeque<u32>,
        responses: Vec<u32>,
        usb_polls: u32,
    }
    let mut state = Transports {
        mailbox: (0..4).collect(),
        responses: Vec::new(),
        usb_polls: 0,
    };
    // An unplugged/non-reading USB host leaves a response or recovery frame
    // pending across every iteration. LAN must still drain its bounded queue.
    for _ in 0..8 {
        let (usb, lan) = embassy_futures::block_on(runtime_poll_control_transports(
            &mut state,
            async |state| {
                state.usb_polls += 1;
                RuntimeUsbInputOutcome {
                    needs_redraw: false,
                    control_command_processed: false,
                    response_pending: true,
                }
            },
            async |state| {
                if let Some(request) = state.mailbox.pop_front() {
                    state.responses.push(request);
                }
                RuntimeLanInputOutcome {
                    needs_redraw: false,
                    command_processed: false,
                }
            },
        ));
        assert!(usb.response_pending);
        assert!(!usb.needs_redraw);
        assert!(!lan.needs_redraw);
        assert!(!lan.command_processed);
    }
    assert_eq!(state.usb_polls, 8);
    assert_eq!(state.responses, [0, 1, 2, 3]);
    assert!(state.mailbox.is_empty());
}

#[test]
fn a_new_usb_command_keeps_lan_queued_until_the_next_iteration() {
    let mut responses = Vec::new();
    for usb_command in [true, false] {
        let (usb, lan) = embassy_futures::block_on(runtime_poll_control_transports(
            &mut responses,
            async |_| RuntimeUsbInputOutcome {
                needs_redraw: true,
                control_command_processed: usb_command,
                response_pending: true,
            },
            async |responses| {
                responses.push(1);
                RuntimeLanInputOutcome {
                    needs_redraw: true,
                    command_processed: true,
                }
            },
        ));
        assert!(usb.needs_redraw);
        assert_eq!(lan.command_processed, !usb_command);
        assert_eq!(lan.needs_redraw, !usb_command);
        assert_eq!(responses.len(), usize::from(!usb_command));
    }
}
