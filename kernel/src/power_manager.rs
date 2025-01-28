use crate::ErrorCode;

pub struct PowerError<T>(pub T, pub ErrorCode);

// Question / TODO: I think the logic for use power will always be the same
// and should not be implemented in the chip specific power manager. We
// should add an enforcement so this is only implemented here (make a trait
// that power manager requires that implements use_power).
pub trait PowerManager<P: Peripheral> {
    // TODO: We should think of what a better name for this might be (since
    // this is really using the state).
    fn use_power_expecting<F, S>(&self, operation: F) -> Result<(), ErrorCode>
    where
        F: FnOnce(S::Reg) -> Result<P::Store, PowerError<P::Store>>,
        S: State<StateEnum = P::StateEnum>,
        <P as Peripheral>::StateEnum: From<<P as Peripheral>::Store>,
    {
        // Obtain the stored state, if there is not a value here, we are within another
        // closure and should return an error.
        let stored_state = self.retrieve_power()?;

        // The stored_state is consumed with the `try_from` operation. For the case of
        // the Any Type, we need to "know" what the original state was. Likewise, we store
        // a copy of the StateEnum to be used for the Any type "recover" operation.
        // TODO: We can make this conditional for only if S contains an Any type. Maybe
        // overkill?
        let any_state_copy = stored_state.copy_store();

        // The function caller denotes the expected state. We obtain the stored state from
        // the power manager. If the stored state is not the expected state, we return an
        // error. In all cases, we must be sure the optionalcell is never empty when we
        // exit this function.
        match S::Reg::try_from(stored_state) {
            // Stored state matches expected state.
            Ok(reg) => {
                // Perform the operation closure provided by the caller.
                let operation_res = operation(reg);

                // TODO: This works for now, but it would be more idiomatic / ergonomic to have
                // some method handling this here so this can be a one liner.
                match operation_res {
                    Ok(new_power) => {
                        self.store_power(new_power.into());
                        Ok(())
                    }

                    Err(PowerError(new_power, e)) => {
                        self.store_power(new_power.into());
                        return Err(e);
                    }
                }
            }
            // Stored state and expected state do not match.
            Err((e, original_state)) => {
                self.store_power(original_state);
                Err(e)
            }
        }
    }

    fn store_power(&self, val: P::StateEnum);
    fn retrieve_power(&self) -> Result<P::StateEnum, ErrorCode>;
}

pub trait Peripheral {
    type StateEnum: StateEnum;
    type Store: Store;
}

pub trait Reg
where
    Self: TryFrom<Self::StateEnum, Error = (ErrorCode, Self::StateEnum)>,
{
    type StateEnum: StateEnum;
}

pub trait Store {}

pub trait StateEnum {
    // The existence of this method destroys all guarantees. We need this
    // to store the anytype, but need to do this in a controlled way so that
    // we don't allow anyone to "escape" the power manager.
    fn copy_store(&self) -> Self;
}

pub trait State {
    type Reg: Reg<StateEnum = Self::StateEnum>;
    type StateEnum: StateEnum;
}

pub trait AnyReg
where
    Self: Reg,
{
}

pub trait SubState {}

pub trait Merge<T> {
    type Output;

    fn merge(self, other: T) -> Self::Output;
}
