use std::time::Duration;

const REFRESH_ATTEMPTS_MAX: usize = 3;
const REFRESH_DELIVERY_WINDOW_MAX: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RefreshDeliveryObservation {
    pub(super) click_deliveries: u64,
    pub(super) request_starts: u64,
    pub(super) duplicate_clicks_blocked: u64,
}

impl RefreshDeliveryObservation {
    pub(super) const fn new(
        click_deliveries: u64,
        request_starts: u64,
        duplicate_clicks_blocked: u64,
    ) -> Self {
        Self {
            click_deliveries,
            request_starts,
            duplicate_clicks_blocked,
        }
    }

    const fn none() -> Self {
        Self::new(0, 0, 0)
    }

    const fn request_started() -> Self {
        Self::new(1, 1, 0)
    }

    pub(super) const fn delivery_started(self) -> bool {
        self.click_deliveries > 0 || self.request_starts > 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RefreshDeliveryAction {
    Click(usize),
    AwaitDelivery,
    AwaitRequest,
    Complete(usize),
    Exhausted(usize),
    DeadlineExceeded(usize),
    Duplicate {
        click_deliveries: u64,
        request_starts: u64,
        duplicate_clicks_blocked: u64,
    },
}

#[derive(Debug, Default)]
pub(super) struct RefreshDelivery {
    attempts_started: usize,
    click_delivered: bool,
    request_started: bool,
}

impl RefreshDelivery {
    pub(super) fn next_action(
        &mut self,
        observation: RefreshDeliveryObservation,
        delivery_window_elapsed: bool,
        budget_remaining: bool,
    ) -> RefreshDeliveryAction {
        self.click_delivered |= observation.click_deliveries > 0;
        self.request_started |= observation.request_starts > 0;

        if observation.duplicate_clicks_blocked > 0
            || observation.click_deliveries > 1
            || observation.request_starts > 1
        {
            return RefreshDeliveryAction::Duplicate {
                click_deliveries: observation.click_deliveries,
                request_starts: observation.request_starts,
                duplicate_clicks_blocked: observation.duplicate_clicks_blocked,
            };
        }
        if self.request_started {
            return RefreshDeliveryAction::Complete(self.attempts_started);
        }
        if !budget_remaining {
            return RefreshDeliveryAction::DeadlineExceeded(self.attempts_started);
        }
        if self.click_delivered {
            return RefreshDeliveryAction::AwaitRequest;
        }
        if self.attempts_started > 0 && !delivery_window_elapsed {
            return RefreshDeliveryAction::AwaitDelivery;
        }
        if self.attempts_started == REFRESH_ATTEMPTS_MAX {
            return RefreshDeliveryAction::Exhausted(self.attempts_started);
        }

        self.attempts_started += 1;
        RefreshDeliveryAction::Click(self.attempts_started)
    }
}

pub(super) fn refresh_delivery_window(budget_remaining: Duration) -> Duration {
    REFRESH_DELIVERY_WINDOW_MAX.min(budget_remaining)
}

#[test]
fn request_start_at_the_delivery_deadline_prevents_another_click() {
    let mut delivery = RefreshDelivery::default();

    assert_eq!(
        delivery.next_action(RefreshDeliveryObservation::none(), true, true),
        RefreshDeliveryAction::Click(1)
    );
    assert_eq!(
        delivery.next_action(RefreshDeliveryObservation::none(), false, true),
        RefreshDeliveryAction::AwaitDelivery
    );
    assert_eq!(
        delivery.next_action(RefreshDeliveryObservation::request_started(), true, true),
        RefreshDeliveryAction::Complete(1)
    );
    assert_eq!(
        delivery.next_action(RefreshDeliveryObservation::none(), true, true),
        RefreshDeliveryAction::Complete(1)
    );
}

#[test]
fn genuinely_undelivered_refresh_clicks_stop_after_three_attempts() {
    let mut delivery = RefreshDelivery::default();

    for attempt in 1..=REFRESH_ATTEMPTS_MAX {
        assert_eq!(
            delivery.next_action(RefreshDeliveryObservation::none(), true, true),
            RefreshDeliveryAction::Click(attempt)
        );
    }
    assert_eq!(
        delivery.next_action(RefreshDeliveryObservation::none(), true, true),
        RefreshDeliveryAction::Exhausted(REFRESH_ATTEMPTS_MAX)
    );
}

#[test]
fn delivered_click_waits_for_its_request_instead_of_retrying() {
    let mut delivery = RefreshDelivery::default();

    assert_eq!(
        delivery.next_action(RefreshDeliveryObservation::none(), true, true),
        RefreshDeliveryAction::Click(1)
    );
    assert_eq!(
        delivery.next_action(RefreshDeliveryObservation::new(1, 0, 0), true, true),
        RefreshDeliveryAction::AwaitRequest
    );
    assert_eq!(
        delivery.next_action(RefreshDeliveryObservation::none(), true, true),
        RefreshDeliveryAction::AwaitRequest
    );
}

#[test]
fn exhausted_budget_never_starts_a_refresh_click() {
    let mut delivery = RefreshDelivery::default();

    assert_eq!(
        delivery.next_action(RefreshDeliveryObservation::none(), true, false),
        RefreshDeliveryAction::DeadlineExceeded(0)
    );
}

#[test]
fn blocked_duplicate_click_is_reported_as_duplicate_evidence() {
    let mut delivery = RefreshDelivery::default();

    assert_eq!(
        delivery.next_action(RefreshDeliveryObservation::none(), true, true),
        RefreshDeliveryAction::Click(1)
    );
    assert_eq!(
        delivery.next_action(
            RefreshDeliveryObservation {
                click_deliveries: 2,
                request_starts: 1,
                duplicate_clicks_blocked: 1,
            },
            true,
            true,
        ),
        RefreshDeliveryAction::Duplicate {
            click_deliveries: 2,
            request_starts: 1,
            duplicate_clicks_blocked: 1,
        }
    );
}
