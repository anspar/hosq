const Migrations = artifacts.require("IpfsManager");

module.exports = function (deployer) {
  deployer.deploy(Migrations);
};