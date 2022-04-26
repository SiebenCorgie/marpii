# MarpII-Command-Graph

Experimental frame-graph helper. Currently, supports chaining multiple sub-passes. The helper handles resource transitions.


Note that the crate and its API is highly experimental. Might change heavily in the future.


### TODOs

- [ ] Allow actual graph
- [ ] Remove "resource manager", instead use stateful resource wrappers
- [ ] Create (optimal?) graph from resource dependencies between `Pass`es. Instead of defining the graph by hand the user can specify a serial pass order and the system finds opportunities for parallel execution.
- [ ] Allow caching graphs. Removes the need to re-record inter-pass dependencies etc.


# Examples
- `test_graph`: The Toy implementation of the dependency solving/submission_list build algorithm. 
