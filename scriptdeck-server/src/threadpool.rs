use std::thread::{JoinHandle, self};
use crossbeam_channel::{Receiver, Sender, unbounded};
use log::info;
use rhai::AST;

enum Runner2Pool {
    Log(String)
}

enum Pool2Runner {
    ByeBye,
}

pub struct RunnerPool {
    max_workers: usize,
    workers: Vec<Runner>,
    receiver: Receiver<Pool2Runner>,
}

impl RunnerPool {
    pub fn new(max_workers: usize) -> RunnerPool {
        if max_workers == 0 {
            panic!("max_workers must be greater than zero!");
        }
        let (tx, rx) = unbounded();

        let mut workers = Vec::with_capacity(max_workers);
        info!("Starting {max_workers} runners");
        for i in 0..max_workers {
            workers.push(Runner::new(i, tx.clone()));
        }

        info!("Shutting down {max_workers} runners");
        for i in 0..max_workers {
            let _ = workers[i].sender.send(Pool2Runner::ByeBye);
        }

        RunnerPool { max_workers, workers: Vec::new(), receiver: rx, }
    }
}

impl Drop for RunnerPool {
    fn drop(&mut self) {
        for i in 0..self.max_workers {
            self.workers[i].sender.send(Pool2Runner::ByeBye);
        }
        for w in self.workers.iter_mut() {
            w.handle.join().unwrap();
        }
    }
}

struct Runner {
    _id: usize,
    handle: JoinHandle<()>,
    sender: Sender<Pool2Runner>,
}

impl Runner {
    fn new(id: usize, sender: Sender<Pool2Runner>) -> Self {
        let (tx, rx) = unbounded();
        let t = thread::spawn(move || {
            info!("Worker {id} started");
            let sender = sender;
            loop {
                let message = rx.recv().unwrap();
                match message {
                    Pool2Runner::ByeBye => {
                        info!("Worker {id} stopped");
                        return;
                    }
                }
            }
        });

        Runner { _id: id, handle: t, sender: tx }
    }
}
