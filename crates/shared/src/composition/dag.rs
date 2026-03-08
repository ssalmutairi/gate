use std::collections::{HashMap, HashSet, VecDeque};

/// Validate that steps form a valid DAG (no cycles) and that all dependencies reference existing steps.
/// Returns an error message if invalid.
pub fn validate_dag(steps: &[(String, Vec<String>)]) -> Result<(), String> {
    let step_names: HashSet<&str> = steps.iter().map(|(name, _)| name.as_str()).collect();

    // Check for duplicate step names
    if step_names.len() != steps.len() {
        return Err("duplicate step names found".to_string());
    }

    // Check all dependencies reference existing steps
    for (name, deps) in steps {
        for dep in deps {
            if !step_names.contains(dep.as_str()) {
                return Err(format!(
                    "step '{}' depends on '{}' which does not exist",
                    name, dep
                ));
            }
        }
        // Check self-dependency
        if deps.contains(name) {
            return Err(format!("step '{}' depends on itself", name));
        }
    }

    // Cycle detection via topological sort (Kahn's algorithm)
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

    for (name, _) in steps {
        in_degree.entry(name.as_str()).or_insert(0);
        adjacency.entry(name.as_str()).or_default();
    }

    for (name, deps) in steps {
        for dep in deps {
            adjacency.entry(dep.as_str()).or_default().push(name.as_str());
            *in_degree.entry(name.as_str()).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<&str> = VecDeque::new();
    for (&name, &deg) in &in_degree {
        if deg == 0 {
            queue.push_back(name);
        }
    }

    let mut processed = 0;
    while let Some(node) = queue.pop_front() {
        processed += 1;
        if let Some(dependents) = adjacency.get(node) {
            for &dep in dependents {
                let deg = in_degree.get_mut(dep).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(dep);
                }
            }
        }
    }

    if processed != steps.len() {
        return Err("dependency cycle detected among steps".to_string());
    }

    Ok(())
}

/// Compute execution levels via topological sort.
/// Each level contains steps that can execute in parallel.
/// Returns levels in execution order.
pub fn compute_levels(steps: &[(String, Vec<String>)]) -> Vec<Vec<String>> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

    for (name, _) in steps {
        in_degree.entry(name.as_str()).or_insert(0);
        adjacency.entry(name.as_str()).or_default();
    }

    for (name, deps) in steps {
        for dep in deps {
            adjacency.entry(dep.as_str()).or_default().push(name.as_str());
            *in_degree.entry(name.as_str()).or_insert(0) += 1;
        }
    }

    let mut levels: Vec<Vec<String>> = Vec::new();
    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();
    queue.sort(); // deterministic order

    while !queue.is_empty() {
        let current_level: Vec<String> = queue.iter().map(|s| s.to_string()).collect();
        let mut next_queue = Vec::new();

        for &node in &queue {
            if let Some(dependents) = adjacency.get(node) {
                for &dep in dependents {
                    let deg = in_degree.get_mut(dep).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        next_queue.push(dep);
                    }
                }
            }
        }

        levels.push(current_level);
        next_queue.sort();
        queue = next_queue;
    }

    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_dag_no_deps() {
        let steps = vec![
            ("a".to_string(), vec![]),
            ("b".to_string(), vec![]),
        ];
        assert!(validate_dag(&steps).is_ok());
    }

    #[test]
    fn valid_dag_linear() {
        let steps = vec![
            ("a".to_string(), vec![]),
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["b".to_string()]),
        ];
        assert!(validate_dag(&steps).is_ok());
    }

    #[test]
    fn valid_dag_diamond() {
        let steps = vec![
            ("a".to_string(), vec![]),
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["a".to_string()]),
            ("d".to_string(), vec!["b".to_string(), "c".to_string()]),
        ];
        assert!(validate_dag(&steps).is_ok());
    }

    #[test]
    fn cycle_detected() {
        let steps = vec![
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
        ];
        let err = validate_dag(&steps).unwrap_err();
        assert!(err.contains("cycle"), "expected cycle error, got: {}", err);
    }

    #[test]
    fn self_dependency() {
        let steps = vec![("a".to_string(), vec!["a".to_string()])];
        let err = validate_dag(&steps).unwrap_err();
        assert!(err.contains("depends on itself"));
    }

    #[test]
    fn missing_dependency() {
        let steps = vec![("a".to_string(), vec!["nonexistent".to_string()])];
        let err = validate_dag(&steps).unwrap_err();
        assert!(err.contains("does not exist"));
    }

    #[test]
    fn duplicate_names() {
        let steps = vec![
            ("a".to_string(), vec![]),
            ("a".to_string(), vec![]),
        ];
        let err = validate_dag(&steps).unwrap_err();
        assert!(err.contains("duplicate"));
    }

    #[test]
    fn compute_levels_parallel() {
        let steps = vec![
            ("a".to_string(), vec![]),
            ("b".to_string(), vec![]),
            ("c".to_string(), vec!["a".to_string(), "b".to_string()]),
        ];
        let levels = compute_levels(&steps);
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].len(), 2); // a and b in parallel
        assert!(levels[0].contains(&"a".to_string()));
        assert!(levels[0].contains(&"b".to_string()));
        assert_eq!(levels[1], vec!["c".to_string()]);
    }

    #[test]
    fn compute_levels_linear() {
        let steps = vec![
            ("a".to_string(), vec![]),
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["b".to_string()]),
        ];
        let levels = compute_levels(&steps);
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["a"]);
        assert_eq!(levels[1], vec!["b"]);
        assert_eq!(levels[2], vec!["c"]);
    }

    #[test]
    fn compute_levels_single() {
        let steps = vec![("a".to_string(), vec![])];
        let levels = compute_levels(&steps);
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0], vec!["a"]);
    }

    #[test]
    fn compute_levels_empty() {
        let steps: Vec<(String, Vec<String>)> = vec![];
        let levels = compute_levels(&steps);
        assert!(levels.is_empty());
    }

    #[test]
    fn three_level_cycle() {
        let steps = vec![
            ("a".to_string(), vec!["c".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["b".to_string()]),
        ];
        let err = validate_dag(&steps).unwrap_err();
        assert!(err.contains("cycle"));
    }
}
