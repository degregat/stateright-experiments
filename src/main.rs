// Copyright 2023 Heliax AG
// SPDX-License-Identifier: GPL-3.0-only

use std::borrow::Cow;

use choice::{choice, Choice};
use stateright::actor::{Actor, ActorModel, ActorModelAction, Envelope, Id, Out};
use stateright::{Checker, Expectation, Model};

use std::fmt::Debug;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

fn main() {
    check_counter_supervisor_by_hand();
    check_counter_supervisor_by_discovery();
}

pub fn check_counter_supervisor_by_hand() {
    println!("Example: Advance Choice Actor State by Hand");

    let model = ActorModel::<choice![SupervisorMachine, CounterMachine], (), u8>::new((), 0)
        .actor(Choice::new(SupervisorMachine {
            initial_state: SupervisorState {
                addr: 0.into(),
                threshold: 3,
                counter_addr: 1.into(),
                success: false,
            },
        }))
        .actor(
            Choice::new(CounterMachine {
                initial_state: CounterState {
                    addr: 1.into(),
                    counter: 0,
                },
            })
            .or(),
        )
        .property(Expectation::Sometimes, "always_true", |_, _state| true);

    println!("model init done");

    println!("Initial state: {:?}", &model.init_states());

    println!("model add state");

    let state1 = model.next_state(
        &model.init_states()[0],
        ActorModelAction::Deliver {
            src: Id::from(0),
            dst: Id::from(1),
            msg: PolyMsg::SupervisorIncrementRequest(3),
        },
    );
    println!("Count up: {:?}", state1.clone().unwrap());
    // CounterState::state.counter == 3

    let state2 = model.next_state(
        &state1.clone().unwrap(),
        ActorModelAction::Deliver {
            src: Id::from(0),
            dst: Id::from(1),
            msg: PolyMsg::SupervisorReportRequest(),
        },
    );
    println!("Request state: {:?}", &state2);

    let state3 = {
        let e: Vec<Envelope<&PolyMsg>> = state2.as_ref().unwrap().network.iter_all().collect();

        model.next_state(
            &state2.as_ref().unwrap(),
            ActorModelAction::Deliver {
                src: e[0].src,
                dst: e[0].dst,
                msg: e[0].msg.clone(),
            },
        ) // Takes the first msg but does not remove it.
    };
    println!("Check supervisor: {:?}", state3);
    // SupervisorState::state.success == true

    model.checker().spawn_bfs().join();
    //model.checker().serve("0:3000");

    println!("advance state by hand: done");
}

pub fn check_counter_supervisor_by_discovery() {
    println!("Example: Assert Discovery");
    let checker = ActorModel::<choice![SupervisorMachine, CounterMachine], (), u8>::new((), 0)
        .actor(Choice::new(SupervisorMachine {
            initial_state: SupervisorState {
                addr: 0.into(),
                threshold: 3,
                counter_addr: 1.into(),
                success: false,
            },
        }))
        .actor(
            Choice::new(CounterMachine {
                initial_state: CounterState {
                    addr: 1.into(),
                    counter: 0,
                },
            })
            .or(),
        )
        .property(Expectation::Sometimes, "always_true", |_, _state| true)
        .checker()
        .spawn_bfs()
        .join();
    //.serve("0:3000");

    println!("{:?}", checker.discoveries());

    println!("checker init done");

    println!("checker assert discovery");
    checker.assert_discovery(
        "always_true",
        vec![
            // Request to increment the counter state of Actor 1 by 3
            ActorModelAction::Deliver {
                src: Id::from(0),
                dst: Id::from(1),
                msg: PolyMsg::SupervisorIncrementRequest(3),
            },
        ],
    );
}

// This trait is used to approximate Mealy Machines, to enable better reasoning about composition, independent of host frameworks.
pub type Address = Id;
pub trait MealyMachine {
    // Input and output message types for this MM. They are roughly equivalent to the Input and Output alphabet.
    type InputMsgs: Clone + Debug + Eq + Hash; // + Serialize + Deserialize once Network support is required
    type OutputMsgs: Clone + Debug + Eq + Hash; // + serde, as above

    // State type of this MM
    type MealyState: Clone + Debug + PartialEq + Hash;

    // Set initial state from struct or default.
    fn initialize(state: Option<Self::MealyState>) -> Self::MealyState;

    // The following functions wraps the state transition and output function of the MM into one.
    // The Set of States is comprised of the valid instantiations of the MealyState struct, which are constrained by the Data Types of its fields.
    fn respond_to_msg(
        _dest: Address,
        _state: Self::MealyState,
        _src: Address,
        _msg: Self::InputMsgs,
    ) -> (Self::MealyState, Vec<(Address, Self::OutputMsgs)>); // A vector containing tuples of (destination, message)
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PolyMsg {
    SupervisorIncrementRequest(CounterSize),
    SupervisorReportRequest(),
    CounterReplyCount(CounterSize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaseActor<T> {
    pub initial_state: T, //T needs to be a Mealy Machine State Type
}

pub type CounterSize = i32;

pub type CounterMachine = BaseActor<CounterState>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct CounterState {
    pub addr: Address,
    pub counter: CounterSize,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum MsgsCounterToSupervisor {
    ReplyCount(CounterSize),
}

impl MealyMachine for CounterMachine {
    type InputMsgs = PolyMsg;
    type OutputMsgs = PolyMsg;
    type MealyState = CounterState;

    fn initialize(state: Option<Self::MealyState>) -> Self::MealyState {
        state.unwrap_or(CounterState {
            addr: 1.into(),
            counter: 0,
        })
    }

    fn respond_to_msg(
        _dest: Address,
        _state: Self::MealyState,
        _src: Address,
        _msg: Self::InputMsgs,
    ) -> (Self::MealyState, Vec<(Address, Self::OutputMsgs)>) {
        println!("CounterMachine response.");

        match _msg {
            Self::InputMsgs::SupervisorIncrementRequest(n) => (
                CounterState {
                    counter: _state.counter + n,
                    .._state
                },
                Vec::new(),
            ),

            Self::InputMsgs::SupervisorReportRequest() => (
                _state,
                vec![(_src, Self::OutputMsgs::CounterReplyCount(_state.counter))],
            ),

            _ => (_state, Vec::new()),
        }
    }
}

impl Actor for CounterMachine {
    type Msg = PolyMsg;
    type State = CounterState;
    type Timer = ();

    fn on_start(&self, _id: Id, _o: &mut Out<Self>) -> Self::State {
        println!("CounterActor initializing.");
        CounterMachine::initialize(Some(self.initial_state))
    }

    fn on_msg(
        &self,
        id: Id,
        state: &mut Cow<Self::State>,
        src: Id,
        msg: Self::Msg,
        o: &mut Out<Self>,
    ) {
        println!("CounterActor on_msg.");
        let (new_state, output_msgs) =
            CounterMachine::respond_to_msg(id, state.as_ref().clone(), src, msg);

        *state.to_mut() = new_state;

        for (addr, msg) in output_msgs {
            o.send(addr, msg)
        }
        println!("CounterActor on_msg done.");
    }
}

// Note: For now the counter is mainly separated into two machines to test interaction.
// TODO: Expand on the supervisor pattern.
pub type SupervisorMachine = BaseActor<SupervisorState>;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum MsgsSupervisorToCounter {
    IncrementRequest(CounterSize), // Increment counter
    ReportRequest(),               // Request a report by the counter
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SupervisorState {
    pub addr: Address,
    pub threshold: CounterSize,
    pub counter_addr: Address,
    pub success: bool,
}

impl MealyMachine for SupervisorMachine {
    type InputMsgs = PolyMsg;
    type OutputMsgs = PolyMsg;

    type MealyState = SupervisorState;

    fn initialize(state: Option<Self::MealyState>) -> Self::MealyState {
        state.unwrap_or(SupervisorState {
            addr: 0.into(),
            threshold: 5,
            counter_addr: 1.into(),
            success: false,
        })
    }

    // TODO: Clarify: Is dest == self_addr?
    fn respond_to_msg(
        _dest: Address,
        _state: Self::MealyState,
        _src: Address,
        _msg: Self::InputMsgs,
    ) -> (Self::MealyState, Vec<(Address, Self::OutputMsgs)>) {
        println!("SupervisorMachine response.");
        match _msg {
            Self::InputMsgs::CounterReplyCount(n) => {
                if n >= _state.threshold {
                    (
                        SupervisorState {
                            success: true,
                            .._state
                        },
                        Vec::new(),
                    )
                } else {
                    (_state, Vec::new())
                }
            }

            _ => (_state, Vec::new()),
        }
    }
}

impl Actor for SupervisorMachine {
    type Msg = PolyMsg;
    type State = SupervisorState;
    type Timer = ();

    fn on_start(&self, _id: Id, _o: &mut Out<Self>) -> Self::State {
        println!("SupervisorMachine initializing.");
        SupervisorMachine::initialize(Some(self.initial_state))
    }

    fn on_msg(
        &self,
        id: Id,
        state: &mut Cow<Self::State>,
        src: Id,
        msg: Self::Msg,
        o: &mut Out<Self>,
    ) {
        println!("SupervisorMachine responding to msg.");

        let (new_state, output_msgs) =
            SupervisorMachine::respond_to_msg(id, state.as_ref().clone(), src, msg);

        *state.to_mut() = new_state;

        for (addr, msg) in output_msgs {
            o.send(addr, msg)
        }
        println!("SupervisorMachine response done.");
    }
}
