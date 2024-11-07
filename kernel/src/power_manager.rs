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
        S: State<Store = P::Store>,
    {
        // Obtain the stored state, if there is not a value here, we are within another
        // closure and should return an error.
        let stored_state = self.retrieve_power()?;

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
                        self.store_power(new_power);
                        Ok(())
                    }

                    Err(PowerError(new_power, e)) => {
                        self.store_power(new_power);
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

    fn store_power(&self, val: P::Store);
    fn retrieve_power(&self) -> Result<P::Store, ErrorCode>;
}

pub trait Peripheral {
    type Store: Store;
}

pub trait Reg
where
    Self: TryFrom<Self::Store, Error = (ErrorCode, Self::Store)>,
{
    type Store: Store;
}

pub trait Store {}

pub trait State {
    type Reg: Reg<Store = Self::Store>;
    type Store: Store;
}
