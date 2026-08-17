use crate::{git::CommitCount, paths::ProjectName, timestamps::MachineTimestamp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushLedgerEntry {
    pub repository_name: ProjectName,
    pub ahead: CommitCount,
    pub checked_at: MachineTimestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushLedger {
    entries: Vec<PushLedgerEntry>,
}

impl PushLedger {
    pub fn record(
        &mut self,
        repository_name: ProjectName,
        ahead: CommitCount,
        checked_at: MachineTimestamp,
    ) {
        self.entries.push(PushLedgerEntry {
            repository_name,
            ahead,
            checked_at,
        });
    }

    pub fn entries(&self) -> &[PushLedgerEntry] {
        &self.entries
    }
}
