 en_join lets you switch from N threads to one thread.
 all threads which reach the en_join point will be shut down, except for the last one.
 the last one gets all the ids from other threads.

 This is useful for map-reduce type computations where the first part of the operation can be
 parallelized, but we need serial execution on the last part;
 for example if you want to sort the returned spans.