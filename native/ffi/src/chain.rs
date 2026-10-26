//! Chaining and inserting answers. The logic (`src/lib/chain.ts`, `src/lib/answer.ts`) lives in the core,
//! `qcalc_core::app::chain` and `app::answer`; this file re-exports it with the record types the FFI uses.

use qcalc_core::app::answer::AnswerForm;
use qcalc_core::app::chain as core_chain;
use serde_json::Value as Json;

use crate::records::ChainAnswer;

pub use qcalc_core::app::answer::{answer_among, insertable_answer, solve_insert, solve_roots};
pub use qcalc_core::app::chain::{ans_written_out, chained_expr, chained_history_expr};
pub use qcalc_core::app::needs::is_graph_command;

/// `chainsFromAnswer`: an operator typed first continues from the last answer; a leading minus never does.
pub fn chains_from_answer(q: &str, last: Option<&ChainAnswer>) -> bool {
    let last = last.map(|l| core_chain::ChainAnswer {
        plain: l.plain.clone(),
        unit: l.unit,
    });
    core_chain::chains_from_answer(q, last.as_ref())
}

/// `insertableHistoryAnswer`, with the exact side when `exact_form`.
pub fn insertable_history_answer(row: &Json, exact_form: bool, sig_figs: f64) -> String {
    let form = if exact_form {
        AnswerForm::Exact
    } else {
        AnswerForm::Approx
    };
    qcalc_core::app::answer::insertable_history_answer(row, form, sig_figs)
}
