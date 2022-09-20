# Resource Tracking Graph (RMG)

This is a resource handling implementation of a multi-queue utilising frame graph.

This is currently highly alpha-state software and has to proofe it self.


# Major TODOs

- rewrite scheduling based on topological sort (currently not optimal)
- finer synchronisation. Currently always waits for whole pipeline in between tasks. With more knowledge 
  could synchronise on a pipeline stage level.
