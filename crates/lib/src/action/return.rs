use super::ActionHandler;

/// The return type of an action.
///
/// This is the type returned by the raw action handler, used internally or when setting the action
/// handler directly via the field on [`Config`](crate::Config). It is not used when setting the
/// action handler via [`Config::on_action`](crate::Config::on_action) and
/// [`Config::on_action_async`](crate::Config::on_action_async) as that takes care of wrapping the
/// return type from the specialised signature on these methods.
pub enum ActionReturn {
	Sync(ActionHandler),
}
