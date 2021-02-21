import sys
from sklearn.linear_model import LinearRegression
from sklearn.model_selection import train_test_split
import pandas as pd
import matplotlib.pyplot as plt
from matplotlib import style
import seaborn as sb
import numpy as np

if len(sys.argv) != 3:
    print('usage: python3 mlr.py DATAFILE TESTSIZE')
    sys.exit(1)

testsize = float(sys.argv[2])

df = pd.read_csv(sys.argv[1])
df.describe()

print('============== page faults + I/O + timer  =====================')
xvar = df[['Increasement in EPT_VIOLATION exit time, us', 'Increasement in I/O time, us', 'Increasement in timer time, us']]
yvar = df['Increasement in execution time, us']

xtrain, xtest, ytrain, ytest = train_test_split(xvar, yvar, test_size = testsize, random_state = 0)

lr = LinearRegression()
lr.fit(xtrain, ytrain)
yhat = lr.predict(xtest)

print('R-Squared: ', lr.score(xtest, ytest))
print('Intercepts: ', lr.intercept_)

sb.distplot(yhat, hist = False, color = 'r', label = 'Predicted Values')
sb.distplot(ytest, hist = False, color = 'b', label = 'Actual Values')
plt.title('Actual vs Predicted Values', fontsize = 16)
plt.xlabel('Values', fontsize = 12)
plt.ylabel('Frequency', fontsize = 12)
plt.legend(loc = 'upper left', fontsize = 13)
plt.savefig('ap.png')

print('============== page faults only =====================')
xvar = df[['Increasement in EPT_VIOLATION exit time, us']]
xtrain, xtest, ytrain, ytest = train_test_split(xvar, yvar, test_size = testsize, random_state = 0)

lr = LinearRegression()
lr.fit(xtrain, ytrain)
yhat = lr.predict(xtest)

print('R-Squared: ', lr.score(xtest, ytest))
print('Intercepts: ', lr.intercept_)

sb.distplot(yhat, hist = False, color = 'r', label = 'Predicted Values')
sb.distplot(ytest, hist = False, color = 'b', label = 'Actual Values')
plt.title('Actual vs Predicted Values', fontsize = 16)
plt.xlabel('Values', fontsize = 12)
plt.ylabel('Frequency', fontsize = 12)
plt.legend(loc = 'upper left', fontsize = 13)
plt.savefig('ap1.png')
