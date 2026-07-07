#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct JobId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobKind {
    Build,
    RunHosted,
    BootKernel,
    DebugLaunch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobMessage {
    pub text: String,
    pub is_error: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdeJob {
    pub id: JobId,
    pub label: String,
    pub kind: JobKind,
    pub state: JobState,
    pub messages: Vec<JobMessage>,
}

#[derive(Clone, Debug, Default)]
pub struct JobManager {
    next_id: u64,
    jobs: Vec<IdeJob>,
}

impl JobManager {
    pub fn start(&mut self, label: impl Into<String>, kind: JobKind) -> JobId {
        let id = JobId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.jobs.push(IdeJob {
            id,
            label: label.into(),
            kind,
            state: JobState::Running,
            messages: Vec::new(),
        });
        id
    }

    pub fn finish(&mut self, id: JobId, state: JobState, message: impl Into<String>) {
        if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
            job.state = state;
            let text = message.into();
            if !text.is_empty() {
                job.messages.push(JobMessage {
                    text,
                    is_error: state == JobState::Failed,
                });
            }
        }
    }

    pub fn jobs(&self) -> &[IdeJob] {
        &self.jobs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_job_lifecycle() {
        let mut jobs = JobManager::default();
        let id = jobs.start("Build shell", JobKind::Build);
        jobs.finish(id, JobState::Succeeded, "ok");

        assert_eq!(jobs.jobs()[0].state, JobState::Succeeded);
        assert_eq!(jobs.jobs()[0].messages[0].text, "ok");
    }
}
