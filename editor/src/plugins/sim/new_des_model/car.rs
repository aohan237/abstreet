use geom::{Distance, Duration, PolyLine, Speed};
use map_model::{Map, Traversable};
use sim::{CarID, DrawCarInput};
use std::collections::VecDeque;

#[derive(Debug)]
pub struct Car {
    pub id: CarID,
    pub vehicle_len: Distance,
    pub max_speed: Option<Speed>,
    // Front is always the current step
    pub path: VecDeque<Traversable>,
    pub end_dist: Distance,
    pub state: CarState,

    // In reverse order -- most recently left is first. The sum length of these must be >=
    // vehicle_len.
    pub last_steps: VecDeque<Traversable>,
}

impl Car {
    pub fn trim_last_steps(&mut self, map: &Map) {
        let mut keep = VecDeque::new();
        let mut len = Distance::ZERO;
        for on in self.last_steps.drain(..) {
            len += on.length(map);
            keep.push_back(on);
            if len >= self.vehicle_len {
                break;
            }
        }
        self.last_steps = keep;
    }

    pub fn get_draw_car(&self, front: Distance, map: &Map) -> DrawCarInput {
        assert!(front >= Distance::ZERO);
        let body = if front >= self.vehicle_len {
            self.path[0]
                .slice(front - self.vehicle_len, front, map)
                .unwrap()
                .0
        } else {
            // TODO This is redoing some of the Path::trace work...
            let mut result = self.path[0]
                .slice(Distance::ZERO, front, map)
                .map(|(pl, _)| pl.points().clone())
                .unwrap_or_else(Vec::new);
            let mut leftover = self.vehicle_len - front;
            let mut i = 0;
            while leftover > Distance::ZERO {
                if i == self.last_steps.len() {
                    panic!("{} spawned too close to short stuff", self.id);
                }
                let len = self.last_steps[i].length(map);
                let start = (len - leftover).max(Distance::ZERO);
                let piece = self.last_steps[i]
                    .slice(start, len, map)
                    .map(|(pl, _)| pl.points().clone())
                    .unwrap_or_else(Vec::new);
                result = PolyLine::append(piece, result);
                leftover -= len;
                i += 1;
            }

            PolyLine::new(result)
        };

        DrawCarInput {
            id: self.id,
            waiting_for_turn: None,
            stopping_trace: None,
            state: match self.state {
                // TODO Cars can be Queued behind a slow Crossing. Looks kind of weird.
                CarState::Queued => sim::CarState::Stuck,
                CarState::Crossing(_, _) => sim::CarState::Moving,
            },
            vehicle_type: self.id.tmp_get_vehicle_type(),
            on: self.path[0],
            body,
        }
    }
}

// TODO These should perhaps be collapsed to (TimeInterval, DistanceInterval, Traversable).
#[derive(Debug)]
pub enum CarState {
    Crossing(TimeInterval, DistanceInterval),
    Queued,
}

#[derive(Debug)]
pub struct TimeInterval {
    pub start: Duration,
    pub end: Duration,
}

impl TimeInterval {
    pub fn percent(&self, t: Duration) -> f64 {
        let x = (t - self.start) / (self.end - self.start);
        assert!(x >= 0.0 && x <= 1.0);
        x
    }
}

#[derive(Debug)]
pub struct DistanceInterval {
    pub start: Distance,
    pub end: Distance,
}

impl DistanceInterval {
    pub fn lerp(&self, x: f64) -> Distance {
        assert!(x >= 0.0 && x <= 1.0);
        self.start + x * (self.end - self.start)
    }
}
