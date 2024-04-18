use std::collections::{HashMap, HashSet};

use crate::{
    core::Container,
    treewalk::types::{
        utils::ResolvedArguments, Dict, DictItems, ExprResult, Function, Str, Tuple,
    },
    types::errors::InterpreterError,
};

use super::Interpreter;

/// This represents a symbol table for a given scope.
#[derive(Debug, PartialEq, Clone, Default)]
pub struct Scope {
    symbol_table: HashMap<String, ExprResult>,

    /// Used to hold directives such as `global x` which will expire with this scope.
    global_vars: HashSet<String>,

    /// Used to hold directives such as `nonlocal x` which will expire with this scope.
    nonlocal_vars: HashSet<String>,
}

impl Scope {
    pub fn new(
        interpreter: &Interpreter,
        function: &Container<Function>,
        arguments: &ResolvedArguments,
    ) -> Result<Container<Self>, InterpreterError> {
        let mut scope = Self::default();

        let function_args = &function.borrow().args;

        // Function expects fewer positional args than it was invoked with and there is not an
        // `args_var` in which to store the rest.
        if function_args.args.len() < arguments.bound_len() && function_args.args_var.is_none() {
            return Err(InterpreterError::WrongNumberOfArguments(
                function_args.args.len(),
                arguments.bound_len(),
                interpreter.state.call_stack(),
            ));
        }

        let bound_args = arguments.bound_args();
        let mut missing_args = vec![];

        for (index, arg_definition) in function_args.args.iter().enumerate() {
            // Check if the argument is provided, otherwise use default
            let value = if index < bound_args.len() {
                bound_args[index].clone()
            } else {
                match &arg_definition.default {
                    Some(default_value) => interpreter.evaluate_expr(default_value)?,
                    None => {
                        missing_args.push(arg_definition.arg.clone());
                        // We use Void here only because if we hit this case, we will return an
                        // error shortly after this loop. We can't do it here because we need to
                        // find all the missing args first.
                        ExprResult::Void
                    }
                }
            };

            scope.insert(&arg_definition.arg, value);
        }

        // Function expects more positional args than it was invoked with.
        if !missing_args.is_empty() {
            let num_missing = missing_args.len();
            let noun = if num_missing == 1 {
                "argument"
            } else {
                "arguments"
            };
            let arg_names = missing_args
                .into_iter()
                .map(|a| format!("'{}'", a))
                .collect::<Vec<_>>()
                .join(" and ");
            let message = format!(
                "{}() missing {} required positional {}: {}",
                function.borrow().name,
                num_missing,
                noun,
                arg_names
            );
            return Err(InterpreterError::TypeError(
                Some(message),
                interpreter.state.call_stack(),
            ));
        }

        if let Some(ref args_var) = function_args.args_var {
            let extra = arguments.len() - function_args.args.len();
            let left_over = bound_args.iter().rev().take(extra).rev().cloned().collect();
            let args_value = ExprResult::Tuple(Container::new(Tuple::new(left_over)));
            scope.insert(args_var.as_str(), args_value);
        }

        if let Some(ref kwargs_var) = function_args.kwargs_var {
            let kwargs_value = ExprResult::Dict(Container::new(Dict::new(arguments.get_kwargs())));
            scope.insert(kwargs_var.as_str(), kwargs_value);
        }

        Ok(Container::new(scope.to_owned()))
    }

    fn from_hash(symbol_table: HashMap<String, ExprResult>) -> Self {
        Self {
            symbol_table,
            global_vars: HashSet::new(),
            nonlocal_vars: HashSet::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<ExprResult> {
        self.symbol_table.get(name).cloned()
    }

    /// Return a list of all the symbols available in this `Scope`.
    pub fn symbols(&self) -> Vec<String> {
        self.symbol_table.keys().cloned().collect()
    }

    pub fn delete(&mut self, name: &str) -> Option<ExprResult> {
        self.symbol_table.remove(name)
    }

    /// Insert an `ExprResult` to this `Scope`. The `Scope` is returned to allow calls to be
    /// chained.
    pub fn insert(&mut self, name: &str, value: ExprResult) -> &mut Self {
        self.symbol_table.insert(name.to_string(), value);
        self
    }

    /// Given a variable `var`, indicate that `var` should refer to the variable in the
    /// global/module scope (which does not live in this struct) for the duration of _this_
    /// local scope.
    pub fn mark_global(&mut self, name: &str) {
        self.global_vars.insert(name.to_string());
    }

    /// Given a variable `var`, indicate that `var` should refer to the variable in the
    /// enclosing scope (which does not live in this struct) for the duration of _this_
    /// local scope.
    pub fn mark_nonlocal(&mut self, name: &str) {
        self.nonlocal_vars.insert(name.to_string());
    }

    pub fn has_global(&self, name: &str) -> bool {
        self.global_vars.contains(name)
    }

    pub fn has_nonlocal(&self, name: &str) -> bool {
        self.nonlocal_vars.contains(name)
    }

    pub fn as_dict(&self) -> Container<Dict> {
        let mut items = HashMap::new();
        for (key, value) in self.symbol_table.iter() {
            items.insert(ExprResult::String(Str::new(key.clone())), value.clone());
        }

        Container::new(Dict::new(items))
    }

    pub fn from_dict(dict: DictItems) -> Self {
        let mut symbol_table = HashMap::new();
        for item in dict.into_iter() {
            let tuple = item.as_tuple().unwrap();
            let key = tuple.first().as_string().unwrap();
            let value = tuple.second();
            symbol_table.insert(key, value);
        }

        Self::from_hash(symbol_table)
    }
}
