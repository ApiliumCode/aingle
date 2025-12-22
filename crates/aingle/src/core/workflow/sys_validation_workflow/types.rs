use super::*;

#[derive(Debug)]
/// The outcome of sys validation
pub(super) enum Outcome {
    /// Moves to app validation
    Accepted,
    /// Moves straight to integration
    SkipAppValidation,
    /// Stays in limbo because another SgdOp
    /// dependency needs to be validated first
    AwaitingOpDep(AnySgdHash),
    /// Stays in limbo because a dependency could not
    /// be found currently on the SGD.
    /// Note this is not proof it doesn't exist.
    MissingSgdDep,
    /// Moves to integration with status rejected
    Rejected,
}
