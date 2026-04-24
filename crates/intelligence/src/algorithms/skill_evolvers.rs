//! Skill evolution using cross-entropy method
//!
//! This algorithm evolves skill parameters to maximize expected reward
//! using the cross-entropy method (CEM).

use rand::prelude::*;

/// A skill with tunable parameters
#[derive(Debug, Clone)]
pub struct Skill {
    /// Skill identifier
    pub id: String,
    /// Current parameter values
    pub params: Vec<f64>,
    /// Parameter bounds (min, max)
    pub bounds: Vec<(f64, f64)>,
}

impl Skill {
    /// Create a new skill with random initial parameters
    pub fn new(id: &str, _num_params: usize, bounds: Vec<(f64, f64)>) -> Self {
        let params = bounds
            .iter()
            .map(|(min, max)| {
                let mut rng = rand::thread_rng();
                rng.sample(rand::distributions::Uniform::new(*min, *max))
            })
            .collect();

        Self {
            id: id.to_string(),
            params,
            bounds,
        }
    }

    /// Get a parameter value
    pub fn get_param(&self, index: usize) -> f64 {
        self.params.get(index).copied().unwrap_or(0.0)
    }

    /// Set a parameter value (clamped to bounds)
    pub fn set_param(&mut self, index: usize, value: f64) {
        if let Some(bound) = self.bounds.get(index) {
            let (min, max) = *bound;
            self.params[index] = value.clamp(min, max);
        }
    }
}

/// Skill population for evolution
#[derive(Debug, Clone)]
pub struct SkillPopulation {
    skills: Vec<Skill>,
    /// Elite percentage (top performers)
    elite_percentage: f64,
    /// Cross-entropy smoothing factor
    smoothing: f64,
}

impl SkillPopulation {
    /// Create a new population with num_skills skills, each with num_params parameters
    pub fn new(num_skills: usize, num_params: usize, bounds: Vec<(f64, f64)>) -> Self {
        let skills = (0..num_skills)
            .map(|i| Skill::new(&format!("skill_{}", i), num_params, bounds.clone()))
            .collect();

        Self {
            skills,
            elite_percentage: 0.2,
            smoothing: 0.7,
        }
    }

    /// Evaluate all skills and return scores
    pub fn evaluate<F>(&mut self, fitness_fn: F) -> Vec<f64>
    where
        F: Fn(&Skill) -> f64,
    {
        self.skills.iter().map(fitness_fn).collect()
    }

    /// Evolve the population based on fitness scores
    pub fn evolve(&mut self, scores: &[f64]) {
        let n = self.skills.len();
        if n == 0 || scores.is_empty() {
            return;
        }

        // Sort indices by score (descending)
        let mut indices: Vec<usize> = (0..n).collect();
        indices.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal));

        // Select elite individuals
        let elite_count = ((n as f64 * self.elite_percentage) as usize).max(1);

        // Calculate new mean and std from elite
        let mut new_means: Vec<f64> = vec![0.0; self.skills[0].params.len()];
        let mut new_stds: Vec<f64> = vec![0.0; self.skills[0].params.len()];

        for &idx in &indices[..elite_count] {
            for (i, &param) in self.skills[idx].params.iter().enumerate() {
                new_means[i] += param / elite_count as f64;
            }
        }

        for &idx in &indices[..elite_count] {
            for (i, &param) in self.skills[idx].params.iter().enumerate() {
                let diff = param - new_means[i];
                new_stds[i] += diff * diff / elite_count as f64;
            }
        }
        new_stds.iter_mut().for_each(|s| *s = s.sqrt().max(0.01));

        // Generate new population
        let mut rng = rand::thread_rng();
        for skill in &mut self.skills {
            for (i, param) in skill.params.iter_mut().enumerate() {
                let mean = new_means[i];
                let std = new_stds[i].max(0.01);

                // Sample from uniform distribution around mean (simplified from normal)
                let low = mean - 2.0 * std;
                let high = mean + 2.0 * std;
                let sample = rng.sample(rand::distributions::Uniform::new(low, high));

                // Smooth towards original value
                *param = self.smoothing * sample + (1.0 - self.smoothing) * *param;

                // Clamp to bounds
                if let Some((min, max)) = skill.bounds.get(i) {
                    *param = param.clamp(*min, *max);
                }
            }
        }
    }

    /// Get the best skill
    pub fn best<F>(&self, fitness_fn: F) -> Option<&Skill>
    where
        F: Fn(&Skill) -> f64,
    {
        self.skills
            .iter()
            .max_by(|a, b| fitness_fn(a).partial_cmp(&fitness_fn(b)).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Get all skills
    pub fn skills(&self) -> &[Skill] {
        &self.skills
    }
}

/// Cross-entropy skill evolver
#[derive(Debug, Clone)]
pub struct SkillEvolver {
    population: SkillPopulation,
    generation: usize,
}

impl SkillEvolver {
    /// Create a new skill evolver
    pub fn new(num_skills: usize, num_params: usize, bounds: Vec<(f64, f64)>) -> Self {
        Self {
            population: SkillPopulation::new(num_skills, num_params, bounds),
            generation: 0,
        }
    }

    /// Evolve for one generation
    pub fn evolve<F>(&mut self, fitness_fn: F)
    where
        F: Fn(&Skill) -> f64,
    {
        let scores = self.population.evaluate(&fitness_fn);
        self.population.evolve(&scores);
        self.generation += 1;
    }

    /// Get current generation
    pub fn generation(&self) -> usize {
        self.generation
    }

    /// Get the best skill
    pub fn best<F>(&self, fitness_fn: F) -> Option<&Skill>
    where
        F: Fn(&Skill) -> f64,
    {
        self.population.best(&fitness_fn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_fitness(skill: &Skill) -> f64 {
        // Maximize sum of parameters
        skill.params.iter().sum()
    }

    #[test]
    fn test_skill_new() {
        let skill = Skill::new("test", 2, vec![(0.0, 1.0), (0.0, 1.0)]);
        assert_eq!(skill.id, "test");
        assert_eq!(skill.params.len(), 2);
    }

    #[test]
    fn test_skill_set_param() {
        let mut skill = Skill::new("test", 2, vec![(0.0, 1.0), (0.0, 1.0)]);
        skill.set_param(0, 5.0); // Should be clamped to 1.0
        assert_eq!(skill.get_param(0), 1.0);
    }

    #[test]
    fn test_skill_population_new() {
        let pop = SkillPopulation::new(5, 3, vec![(0.0, 1.0); 3]);
        assert_eq!(pop.skills.len(), 5);
    }

    #[test]
    fn test_skill_population_evolve() {
        let mut pop = SkillPopulation::new(5, 3, vec![(0.0, 10.0); 3]);
        let scores = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        pop.evolve(&scores);
        // Just verify it doesn't panic
    }

    #[test]
    fn test_skill_evolver_generation() {
        let mut evolver = SkillEvolver::new(5, 3, vec![(0.0, 1.0); 3]);
        assert_eq!(evolver.generation(), 0);
        evolver.evolve(simple_fitness);
        assert_eq!(evolver.generation(), 1);
    }
}