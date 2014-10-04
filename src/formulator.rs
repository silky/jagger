use std::collections::{TreeMap, BitvSet};

use std::rc::Rc;
use std::fmt;
use std::vec;
use solver;
use solver::{Rule, RuleRef, Expression, Solution, SolutionValue, True, False, Unassigned, Term, Not, Lit};

use pkg;
use pkg::{Package, Gte, Lt, Lte, Eq, Any};

// ----------------------------------------------------------------------------
//
// ----------------------------------------------------------------------------

enum FormulatorError {
    NoVariableFor (String, uint)
}

impl fmt::Show for FormulatorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            NoVariableFor (ref name, ordinal) => {
                write!(f, "{}, #{:u}", name, ordinal)
            }
        }
    }
}

type FormulatorResult<T> = Result<T, FormulatorError>;

fn no_var_for<T>(pkg: &Package) -> FormulatorResult<T> {
    Err(NoVariableFor(String::from_str(pkg.name()), pkg.ordinal()))
}

/**
 *
 */
struct Solver<'a> {
    pkgvars: TreeMap<&'a pkg::Package, uint>,
    pkgdb: &'a pkg::PkgDb
}

impl<'a> Solver<'a> {
	fn new(packages: &'a pkg::PkgDb) -> Solver<'a> {
		let mut s = Solver{ pkgvars: TreeMap::new(), pkgdb: packages };
        for (n, p) in packages.iter().enumerate() {
            s.pkgvars.insert(p, n);
        }
        s
	}

    /**
     * Generates a rule descrbing a mutual exclusion
     */
    fn make_conflict_rule(&self, a: &Package, b: &Package) -> FormulatorResult<Rule> {
        let va = Not(try!(self.pkg_var(a)));
        let vb = Not(try!(self.pkg_var(b)));
        Ok(Rule(vec![va, vb]))
    }

    fn pkg_vars(&self, name: &str, exp: pkg::VersionExpression) -> FormulatorResult<Vec<uint>> {
        let mut pkgs : Vec<uint> = vec![];
        for pkg in self.pkgdb.select(name, exp).iter() {
            let pkgvar = try!(self.pkg_var(*pkg));
            pkgs.push(pkgvar);
        }
        Ok(pkgs)
    }

    fn apply_system_rules(&mut self) {

    }

    /**
     * Looks up the variable that represents a given package in the solver. 
     * Returns None if no such variable exists.
     */
    fn find_pkg_var(&'a self, pkg: &'a pkg::Package) -> Option<uint> {
        match self.pkgvars.find(&pkg) {
            Some(n) => Some(*n),
            None => None
        }
    }

    fn pkg_var(&'a self, pkg: &'a pkg::Package) -> FormulatorResult<uint> {
        match self.find_pkg_var(pkg) {
            None => no_var_for(pkg),
            Some(n) => Ok(n)
        }  
    }
}

impl<'a> fmt::Show for Solver<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (key, value) in self.pkgvars.iter() {
            try!(writeln!(f, "{}: {:p}: {}", key, *key, value));
        }
        writeln!(f, "")
    }    
}

/**
 * Constructs a set of rules that say only one package with the supplied 
 * name may be installed. For example, should you have a repo with packages 
 * A1, A2 & A3, then this function should return the ruleset of 
 *  (~A1 | ~A2) & (~A1 | ~A3) & (~A2 | ~A3)
 */
fn make_unique_install_clauses(s: &Solver, name: &str) -> FormulatorResult<Vec<Rule>> {
    let mut result = vec![];
    let mut visited = BitvSet::new();
    let pkgs = try!(s.pkg_vars(name, Any));

    for a in pkgs.iter() {
        for b in pkgs.iter() {
            if *a == *b { 
                continue;
            }

            if visited.contains(b) { 
                continue;
            }

            result.push( Rule(vec![Not(*a), Not(*b)]) );
        }
        visited.insert(*a);
    }
    Ok(result)
}

#[test]
fn unique_package_install_rules_are_created_correctly() {
    // asserts that the rules stating that only one version of a package may be
    // installed are created as we expect (i.e., if packages A1, A2 and A3
    // exist, then we want to have rules like (~A1 | ~A2) & (~A2 | ~A3) & (~A1 | ~A3)

    let db = &mk_test_db();
    let s = Solver::new(db);
    let actual = make_unique_install_clauses(&s, "alpha").unwrap();
    let pkgs = db.select("alpha", Any);

    for a in pkgs.iter() {
        for b in pkgs.iter() {
            if a != b {
                // assert that we can find a clause that says (~a | ~b) or 
                // (~b | ~a)
                assert!( actual.iter().find(|r| {
                    let fwd = s.make_conflict_rule(*a, *b).unwrap();
                    let rev = s.make_conflict_rule(*b, *a).unwrap();

                    (*r).eq(&fwd) || (*r).eq(&rev)
                }).is_some())
            }
        }
    }

    // assert that there are the number of clauses that we would expect
    let n = (pkgs.len()-1) * pkgs.len() / 2;
    assert!(actual.len() == n);
}

/**
 * Generates a list of conflicts
 */
fn make_conflicts_clauses(s: &Solver, pkg: &pkg::Package, exp: &pkg::PkgExp) -> FormulatorResult<Vec<Rule>> {
    let pkgvar = try!(s.pkg_var(pkg));
    let mut result = Vec::new();

    for conflict in s.pkgdb.select_exp(exp).iter() {
        let conflict_var = try!(s.pkg_var(*conflict));
        result.push( Rule::from([Not(pkgvar), Not(conflict_var)]) ) 
    }
    Ok(result)
}

#[test]
fn package_conflict_rules_are_generated_correctly() {
    let db = &mk_test_db();

    // find the package that we want to test
    let pkg = match db.select("gamma", Eq(5)).as_slice() {
        [p] => p,
        _ => {
            assert!(false, "Expected exactly one package returned from select");
            return
        }
    };

    let s = Solver::new(db);
    let pkgvar = s.pkg_var(pkg).unwrap();
    let expected : Vec<Rule> = db.select("alpha", Lte(2))
                                 .iter()
                                 .map(|p| { 
                                    Rule( vec![Not(pkgvar), Not(s.pkg_var(*p).unwrap())] )
                                 })
                                 .collect();

    match make_conflicts_clauses(&s, pkg, &pkg.conflicts()[0]) {
        Ok (actual) => {
            println!("actual: {}", actual);
            println!("expected: {}", expected);
            assert!(actual == expected);
        },
        Err (reason) => {
            assert!(false, "Failed with {}", reason)
        }
    }
}

/**
 * Generates rules that specify that a version of the installed packages must 
 * stay installed. Installed packages can be upgraded but not uninstalled.
 */
fn make_installed_package_upgrade_rules(s: &Solver) -> FormulatorResult<Vec<RuleRef>> {
    let mut result = Vec::new();
    for pkg in s.pkgdb.installed_packages().iter() {
        let valid_pkgs = s.pkgdb.select(pkg.name(), Gte(pkg.ordinal()));
        let mut rule = Rule::new();
        for p in valid_pkgs.iter() {
            rule.add(Lit(try!(s.pkg_var(*p))))
        }
        result.push(Rc::new(rule));
    }
    Ok(result)
}

#[test]
fn installed_packages_must_be_installed_or_upgraded() {
    // asserts that the rules stating that a package's dependencies  

    let db = &mk_test_db();
    let s = Solver::new(db);

    let mk_test_rule = |name, ord| -> Rule {
        FromIterator::from_iter(
            db.select(name, Gte(ord))
              .iter()
              .map(|p| s.pkg_var(*p).unwrap())
              .map(|v| Lit(v)))
    };

    let find_rule = |a: &Rule, b: &Rule| -> bool {
        let Rule(ref r1) = *a;
        let Rule(ref r2) = *b;
        r1 == r2
    };

    match make_installed_package_upgrade_rules(&s) {
        Ok (rules) => {
            assert!(rules.len() == 2, "Expected 2 rules, got {}", rules.len());

            let r1 = mk_test_rule("alpha", 1);
            let r2 = mk_test_rule("beta", 4);

            assert!(rules.iter()
                         .find(|x| find_rule(x.deref(), &r1))
                         .is_some(), 
                    "Couldn't find rule {}", r1);

            assert!(rules.iter()
                         .find(|x| find_rule(x.deref(), &r2))
                         .is_some(), 
                    "Couldn't find rule {}", r2);

        },
        Err (reason) => {
            fail!("failed with {}", reason)
        }
    }
}

/**
 * Automatically deselects all packages older than any installed packages.
 */
fn deny_installed_package_downgrades(s: &Solver,sln: &Solution) -> FormulatorResult<Solution> {
    let mut result = sln.clone(); 
    for pkg in s.pkgdb.installed_packages().iter() {
        let invalid_pkgs = s.pkgdb.select(pkg.name(), Lt(pkg.ordinal()));
        for invalid_pkg in invalid_pkgs.iter() {
            let ivar = try!(s.pkg_var(*invalid_pkg));
            result.set(ivar, False);
        }
    };
    Ok (result)
}

#[test]
fn installed_package_downgrades_are_disabled() {
    // asserts that the variables that indicate an installed package downgrade 
    // have been set to false by the appropriate function.
    let db = &mk_test_db();
    let solver = Solver::new(db);
    let sln = deny_installed_package_downgrades(&solver, &Solution::new()).unwrap();

    println!("Pkgs: {}", solver.pkgvars);
    println!("Soln: {}", sln);

    for pkg in db.installed_packages().iter() {
        let pvar = solver.pkg_var(*pkg).unwrap();
        assert!(sln.get(pvar) == Unassigned);

        for p in db.iter() {
            let v = solver.pkg_var(p).unwrap();
            if p.name() == pkg.name() {
                if p.ordinal() < pkg.ordinal() {
                    assert!(sln.get(v) == False)
                }
                else {
                    assert!(sln.get(v) == Unassigned)
                }
            }
        }
    }
}

/**
 * Generates a rule requiring that at least one of the packages specified 
 * by the package expression must be installed for the root package is 
 * installed. For example, given package A depends on package B, 
 * versions 2-4, this method will generate a rule like
 *
 *    (~A | B2 | B3 | B4)
 *
 * This rule basically states that either A is not installed, or package A 
 * AND any of B2, B3, B4 are installed. We rely on the clauses generated by 
 * the unique install rule to make sure *only one* of them is installed in 
 * the end.
 */
fn make_requires_clause(s: &Solver, pkg: &pkg::Package, exp: &pkg::PkgExp) -> FormulatorResult<Rule> {
    let mut result = Vec::new();
    let pkgvar = try!(s.pkg_var(pkg));
    result.push(Not(pkgvar));
    for dep in s.pkgdb.select_exp(exp).iter() {
        let v = try!(s.pkg_var(*dep));
        result.push(Lit(v))
    }
    Ok(Rule(result))
}


#[test]
fn package_requirement_rules_are_created_correctly() {
    // asserts that the rules stating that a package's dependencies  
    let db = &mk_test_db();

    // find the package that we want to test
    let pkg = match db.select("gamma", Eq(5)).as_slice() {
        [p] => p,
        _ => {
            assert!(false, "Expected exactly one package returned from select");
            return
        }
    };

    let s = Solver::new(db);

    let mut expected = vec![ match s.pkg_var(pkg) {
        Err(reason) => { assert!(false, "Failed with {}", reason); return },
        Ok(var) => Not(var)
    }];
    match s.pkg_vars("beta", Gte(5)) {
        Err(reason) => assert!(false, "Failed with {0}", reason),
        Ok(vars) => expected.extend(vars.iter().map(|x| Lit(*x)))
    };

    match make_requires_clause(&s, pkg, &pkg.requires()[0]) {
        Ok (actual) => {
            println!("actual: {}", actual);
            println!("expected: {}", expected);
            assert!(actual == Rule(expected));
        },
        Err (reason) => {
            assert!(false, "Failed with {}", reason)
        }
    }
}


#[cfg(test)]
fn mk_test_db() -> pkg::PkgDb {
    // b0  b1  b2  b3  b4* b5  b6  b7  b8  b9
    //  |   |   |   |   |   |   |   |   |   |
    // >=  >=  >=  >=  >=  >=  >=  >=  >=  >=
    //  |   |   |   |   |   |   |   |   |   |
    // g0  g1  g2  g3  g4  g5  g6  g7  g8  g9
    //  | /     | /     | /     | /     | /
    //  <=      <=      <=      <=      <=
    //  |       |       |       |       |
    // a0      a1*     a2      a3      a4

    let mut db = pkg::PkgDb::new();
    db.add_packages(Vec::from_fn(5, |n| { 
        let state = if n == 1 { pkg::Installed } else { pkg::Available };
        pkg::Package::new("alpha", n, state)
    }).as_slice());
    
    db.add_packages(Vec::from_fn(10, |n| {
        let state = if n == 4 { pkg::Installed } else { pkg::Available };
        pkg::Package::new("beta", n, state)
    }).as_slice());
    
    db.add_packages(Vec::from_fn(10, |n| { 
        let mut p = pkg::Package::new("gamma", n, pkg::Available);
        p.add_requirement("beta", Gte(n));
        p.add_conflict("alpha", Lte(n / 2));
        p
    }).as_slice());


    return db
}