# Resource Tracking Graph (RMG)

## Introduction

Resource and scheduling framework for MarpII. Every _action_ is defined as a _Task_ on a common timeline. RMG handles the scheduling over several queues, which _might_ result in async-compute or dedicated transfer-queue usage if data-dependencies permit it.

All resource binding is done in one of 5 descriptor sets via a _bindless_ philosophy.
The set usages are:

- 0: Buffers
- 1: StorageImages
- 2: SampledImages
- 3: Samplers
- 4: AccelerationStructures

Take a look at the examples, or write an issue for better documentation if you're actually interested ;).

## Validation

Set `RMG_VALIDATE=1` to load validation layers.
