use crate::managers::{ManagerStats, PackageManager, pacman::FetchPacmanStats};

pub fn get_manager_stats() -> ManagerStats {
    let backend = FetchPacmanStats;
    backend.get_stats()
}
